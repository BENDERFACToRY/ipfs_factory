use std::{
    fs::read_dir, fs::File, path::Path, path::PathBuf, process::Command, process::Stdio,
    time::Duration,
};

use anyhow::bail;
use clap::{App, Arg};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use valico::json_schema;

fn convert_to_vorbis(input: &Path, output: &Path) -> Result<(), anyhow::Error> {
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
struct MediaInfo {
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

#[derive(Serialize)]
struct HBContext {
    title: String,
    tos: String,
    torrent: String,
    files: Vec<AudioFileHB>,
}

#[derive(Serialize)]
struct AudioFileHB {
    filename_url: String,
    filename: String,
    duration: String,
    flac_size: String,
    ogg_size: String,
    format: String,
}

impl From<&AudioFile> for AudioFileHB {
    fn from(af: &AudioFile) -> Self {
        AudioFileHB {
            filename_url: af.filename().replace(' ', "%20"),
            filename: af.filename(),
            format: af.format_str.clone(),
            duration: {
                let sec = af.duration.as_secs();
                if sec <= 59 {
                    format!("{}s", sec)
                } else {
                    let min = (sec as f32 / 60.0).floor() as u64;
                    let sec = sec - (min * 60);
                    format!("{}m {}s", min, sec)
                }
            },
            flac_size: format!("{}MB", af.orig_size_bytes / 1024 / 1024),
            ogg_size: format!("{}MB", af.ogg_size_bytes / 1024 / 1024),
        }
    }
}

// handlebars_helper!(filename: |v: u32| f.filename());

fn write_season_index(root: &Path, tos: String) -> Result<(), anyhow::Error> {
    let reg = Handlebars::new();

    let template = std::fs::read_to_string("handlebars/season_index.hbs")?;

    let output = File::create(root.join("index.html"))?;

    let ctx = HBContext {
        title: "Season 2 index".to_owned(),
        torrent: "".to_string(),
        tos: tos.replace(' ', "%20"),
        files: Vec::new(),
    };

    reg.render_template_to_write(&template, &ctx, output)?;

    Ok(())
}

fn write_index(
    root: &Path,
    files: &[AudioFile],
    tos: String,
    title: String,
    torrent: String,
) -> Result<(), anyhow::Error> {
    let reg = Handlebars::new();

    let template = std::fs::read_to_string("handlebars/day_index.hbs")?;

    let output = File::create(root.join("index.html"))?;

    let ctx = HBContext {
        title,
        tos: tos.replace(' ', "%20"),
        torrent: torrent.replace(' ', "%20"),
        files: files.iter().map(From::from).collect(),
    };

    reg.render_template_to_write(&template, &ctx, output)?;

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
                println!(
                    "  {} already exists, skipping conversion",
                    ogg_path.display()
                );
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

    write_index(
        &root,
        &files,
        tos.file_name().unwrap().to_string_lossy().to_string(),
        root.canonicalize()?
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        torrent.file_name().unwrap().to_string_lossy().to_string(),
    )?;

    let season_tos = tos
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(tos.file_name().unwrap());
    println!(
        "Copying ToS from {} to {}",
        tos.display(),
        season_tos.display()
    );
    std::fs::copy(tos, &season_tos)?;

    write_season_index(
        root.parent().unwrap(),
        season_tos
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    )?;

    Ok(())
}

fn get_validated_json(json_path: &Path) -> Result<serde_json::Value, anyhow::Error> {
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

fn main() -> Result<(), anyhow::Error> {
    let matches = App::new("cb_processor")
        .version("0.0.1")
        .arg(
            Arg::with_name("input")
            .short("i")
            .long("input")
            .takes_value(true)
            .required(true)
            .help("Path to season.json")
        )
        .arg(
            Arg::with_name("data-dir")
                .short("d")
                .long("data")
                .takes_value(true)
                .required(true)
                .help("Path to data directory")
                .long_help("Path to data directory\n\nThis is the directory containing the files references in the recordings json file")
        )
        .get_matches();

    let season_json_path = matches.value_of("input").expect("Missing --input argument");
    let season_json_path = Path::new(season_json_path);

    if !season_json_path.exists() {
        bail!("Input file {} does not exist", season_json_path.display());
    }

    let season_json = get_validated_json(season_json_path)?;

    for root in matches.values_of("dir").unwrap() {
        let root = Path::new(root);
        process(root)?;
    }

    Ok(())
}
