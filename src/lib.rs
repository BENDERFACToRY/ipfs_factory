use std::io::Write;
use std::{
    collections::HashSet,
    fs::read_dir,
    fs::File,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::bail;
use colored::Colorize;
use serde::Deserialize;
use serde_json::Value;
use types::{Recording, RecordingInner, Season};
use valico::json_schema;

pub mod types;

pub fn get_validated_json(json_path: &Path) -> Result<serde_json::Value, anyhow::Error> {
    let file = File::open(json_path)?;
    let json: Value = serde_json::from_reader(file)?;

    if let Value::Object(map) = &json {
        if let Some(Value::String(schema)) = map.get("$schema") {
            if schema.starts_with("./") {
                // local file, fine it relative to json_path
                let schema_path = json_path.parent().unwrap().join(schema);
                let schema_file = File::open(schema_path)?;
                let schema_json = serde_json::from_reader(schema_file)?;

                let mut scope = json_schema::Scope::new();
                let schema = scope.compile_and_return(schema_json, false).unwrap();
                let res = schema.validate(&json);
                if res.is_valid() {
                    return Ok(json);
                } else {
                    bail!("JSON not valid, schema validation failed: {:?}", res)
                }
            }
        }
    }

    // no schema found, just return it unvalidated
    return Ok(json);
}

pub fn convert_to_vorbis(input: &Path, output: &Path) -> Result<(), anyhow::Error> {
    let mut ffmpeg = Command::new("ffmpeg")
        .arg("-i")
        .arg(input)
        .arg(output)
        .stdout(Stdio::null())
        .spawn()?;

    let exit_status = ffmpeg.wait()?;
    if exit_status.success() {
        Ok(())
    } else {
        bail!("ffmpeg returned {:?}", exit_status)
    }
}

#[derive(Debug)]
struct AudioFile {
    pub orig_path: PathBuf,
    pub orig_size_bytes: u64,
    pub ogg_path: PathBuf,
    pub ogg_size_bytes: u64,
    pub duration: Duration,
    pub format_str: String,
}
impl AudioFile {
    pub fn filename(&self) -> String {
        format!("{}", self.orig_path.file_stem().unwrap().to_string_lossy())
    }
}

// #[derive(Deserialize)]
// struct MediaInfoTrack {
//     #[serde(rename = "Duration")]
//     pub duration: String
// }
#[derive(Deserialize, Debug, Clone)]
pub struct MediaInfo {
    #[serde(rename = "@type")]
    pub t: String,
    #[serde(rename = "Format")]
    pub format: String,
    #[serde(rename = "Channels")]
    pub channels: String,
    #[serde(rename = "SamplingRate")]
    pub sample_rate: String,
    #[serde(rename = "BitDepth")]
    pub bit_depth: String,
    #[serde(rename = "Duration")]
    pub duration: String,
}

impl MediaInfo {
    /// Get technical info about a piece of media
    pub fn new<P: AsRef<Path>>(path: P) -> Result<MediaInfo, anyhow::Error> {
        let path = path.as_ref();

        let mediainfo = Command::new("mediainfo")
            .arg("--Output=JSON")
            .arg(path)
            .stdout(Stdio::piped())
            .spawn()?;

        let output = mediainfo.wait_with_output()?;

        let output = String::from_utf8_lossy(&output.stdout);

        let json: Value = serde_json::from_str(&output)?;

        if let Value::Object(mut map) = json {
            if let Some(Value::Object(mut map)) = map.remove("media") {
                if let Some(Value::Array(arr)) = map.remove("track") {
                    for arr in arr {
                        if let Value::Object(ref obj) = arr {
                            if obj
                                .get("@type")
                                .and_then(|obj| obj.as_str())
                                .map_or(false, |s| s == "Audio")
                            {
                                let media_info: MediaInfo = serde_json::from_value(arr)?;
                                return Ok(media_info);
                            }
                        }
                    }
                }
            }
        }

        bail!("Failed to find media info data")
    }
}

use askama::Template;

#[derive(Template)]
#[template(path = "season_index.html")]
pub struct SeasonIndexTemplate<'a> {
    season: &'a Season,
    tag_set: HashSet<&'a str>,
}

#[derive(Template)]
#[template(path = "recording_index.html")]
pub struct RecordingIndexTemplate<'a> {
    season: &'a Season,
    recording: &'a Recording,
}

// impl From<&AudioFile> for AudioFileHB {
//     fn from(af: &AudioFile) -> Self {
//         AudioFileHB {
//             filename_url: af.filename().replace(' ', "%20"),
//             filename: af.filename(),
//             format: af.format_str.clone(),
//             duration: {
//                 let sec = af.duration.as_secs();
//                 if sec <= 59 {
//                     format!("{}s", sec)
//                 } else {
//                     let min = (sec as f32 / 60.0).floor() as u64;
//                     let sec = sec - (min * 60);
//                     format!("{}m {}s", min, sec)
//                 }
//             },
//             flac_size: format!("{}MB", af.orig_size_bytes / 1024 / 1024),
//             ogg_size: format!("{}MB", af.ogg_size_bytes / 1024 / 1024),
//         }
//     }
// }

// handlebars_helper!(filename: |v: u32| f.filename());

pub fn write_season_index(season: &Season, output_root: &Path, data_dir: &Path) -> Result<(), anyhow::Error> {
    let mut tag_set = HashSet::new();
    for rec in &season.recordings {
        for tag in &rec.tags {
            tag_set.insert(tag.as_ref());
        }
        // tag_set.extend(rec.tags.as_ref());
    }

    let context = SeasonIndexTemplate { season, tag_set };

    let f = output_root.join("index.html");
    let mut output = File::create(&f)?;

    let rendered: String = context.render()?;
    output.write_all(rendered.as_bytes())?;

    println!("Write season index to {}", f.display());

    Ok(())
}

pub fn write_all_recording_index(season: &Season, output_root: &Path, _data_dir: &Path) -> Result<(), anyhow::Error> {
    for recording in &season.recordings {
        let context = RecordingIndexTemplate { season, recording };

        let f = output_root.join(&recording.data_folder).join("index.html");
        let mut output = File::create(&f)?;

        let rendered: String = context.render()?;
        output.write_all(rendered.as_bytes())?;

        println!("Wrote recording index to {}", f.display());
    }

    Ok(())
}

fn process(root: &Path) -> Result<(), anyhow::Error> {
    if !root.exists() {
        bail!("Directory {:?} doesn't exist!", root);
    }

    println!("Processing {}", root.display());

    let _ = std::fs::create_dir(root.join("ogg"));

    let mut files = Vec::new();
    let mut tos = None;
    let mut torrent = None;

    println!("Scanning {}", root.display());
    for file in read_dir(root)? {
        let file = file?;
        let file_path = file.path();
        if file_path.extension().map_or(false, |e| e == "flac") {
            // convert this to ogg
            let ogg_path = root
                .join("ogg")
                .join(file_path.file_name().unwrap())
                .with_extension("ogg");
            if ogg_path.exists() {
                println!("  {} already exists, skipping conversion", ogg_path.display());
            } else {
                println!("  {} converting to ogg....", ogg_path.display());
                convert_to_vorbis(&file_path, &ogg_path)?;
            }

            let media_info = MediaInfo::new(&file_path)?;
            println!("{:#?}", media_info);
            let duration: f32 = media_info.duration.parse()?;

            let sample_rate: f32 = media_info.sample_rate.parse()?;

            files.push(AudioFile {
                orig_path: file_path.to_owned(),
                orig_size_bytes: file.metadata()?.len(),
                ogg_path: ogg_path.to_owned(),
                ogg_size_bytes: std::fs::metadata(ogg_path)?.len(),
                duration: Duration::from_secs(duration.floor() as u64),
                format_str: format!(
                    "{}ch {:.1}kHz {}bit",
                    media_info.channels,
                    sample_rate / 1000.0,
                    media_info.bit_depth
                ),
            })
        }
        if file_path.extension().map_or(false, |e| e == "txt") {
            tos = Some(file_path.clone());
        }
        if file_path.extension().map_or(false, |e| e == "torrent") {
            torrent = Some(file_path.clone());
        }
    }

    let tos = tos.expect("No TOS file found");
    let torrent = torrent.expect("No torrent file found");

    files.sort_by_key(|f| f.filename());

    // write_index(
    //     &root,
    //     &files,
    //     tos.file_name().unwrap().to_string_lossy().to_string(),
    //     root.canonicalize()?
    //         .file_name()
    //         .unwrap()
    //         .to_string_lossy()
    //         .to_string(),
    //     torrent.file_name().unwrap().to_string_lossy().to_string(),
    // )?;

    let season_tos = tos.parent().unwrap().parent().unwrap().join(tos.file_name().unwrap());
    println!("Copying ToS from {} to {}", tos.display(), season_tos.display());
    std::fs::copy(tos, &season_tos)?;

    // write_season_index(
    //     root.parent().unwrap(),
    //     season_tos
    //         .file_name()
    //         .unwrap()
    //         .to_string_lossy()
    //         .to_string(),
    // )?;

    Ok(())
}

/// Returns the number of errors found
pub fn validate_and_print(json_path: &Path, data_dir: &Path) -> anyhow::Result<usize> {
    let mut errors = 0;

    let json_root = json_path.parent().unwrap();

    let season = get_validated_json(json_path)?;
    let season: types::SeasonInner = serde_json::from_value(season)?;

    // let mut stdout = StandardStream::stdout(colors);

    // stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    // writeln!(stdout, "Checking Season {:?}:", season.title)?;
    // stdout.reset()?;
    println!("Checking season {}:", season.title.green());

    // println!("{:#?}", season);

    for recording in season.recordings {
        println!("\n  Reading recording {}...", recording.yellow());
        let recording = get_validated_json(&json_root.join(recording))?;
        let recording: RecordingInner = serde_json::from_value(recording)?;

        // each recording specifies their own local data folder relative to the global data_root
        let data_dir = data_dir.join(recording.data_folder);

        let stereo_mix = data_dir.join(&recording.stereo_mix.vorbis);
        if !stereo_mix.exists() {
            println!(
                " {}: Stereo mix file doesn't exist {}",
                "ERROR".red(),
                format!("{}", stereo_mix.display()).yellow()
            );
            errors += 1;
        } else {
            println!("  {} Stereo mix", "OK".green());
        }

        let torrent_file = data_dir.join(&recording.torrent);
        if !torrent_file.exists() {
            println!(
                " {}: torrent file doesn't exist {}",
                "ERROR".red(),
                format!("{}", stereo_mix.display()).yellow()
            );
            errors += 1;
        } else {
            println!("  {} torrent file", "OK".green());
        }

        println!("  Tracks for {}:", recording.title.cyan());

        // println!("{:#?}", recording);

        for track in &recording.tracks {
            println!("    Checking track {}", format!("{}", track.id).cyan());
            let flac_path = data_dir.join(&track.flac);
            if !flac_path.exists() {
                println!(
                    "      {}: Flac file for `{}` track {} does not exist ({})",
                    "ERROR".red(),
                    recording.title,
                    track.id,
                    flac_path.display()
                );
                errors += 1;
            } else {
                println!("      {} Flac orginal", "OK".green());
            }

            let ogg_path = data_dir.join(&track.vorbis);
            if !ogg_path.exists() {
                println!(
                    "      {}: OGG Vorbis file for `{}` track {} does not exist ({})",
                    "ERROR".red(),
                    recording.title,
                    track.id,
                    ogg_path.display()
                );
                errors += 1;
            } else {
                println!("      {} Ogg vorbis", "OK".green());
            }
        }
    }

    Ok(errors)
}
