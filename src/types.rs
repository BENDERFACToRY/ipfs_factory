use std::{borrow::Cow, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};

use crate::MediaInfo;

#[derive(Deserialize, Debug)]
/// This is the raw JSON struct
pub(crate) struct SeasonInner {
    #[serde(rename = "$schema")]
    schema: String,
    pub title: String,
    pub recordings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Season {
    pub title: String,
    pub recordings: Vec<Recording>,

    //pub(crate) ondisk_root: PathBuf,
}

impl Season {
    pub fn load<P: AsRef<Path>>(json: P, ondisk_root: Option<&Path>, cache: Option<&Season>) -> Result<Self, anyhow::Error> {
        let json = json.as_ref();
        let json_root = json.parent().unwrap();

        let inner = crate::get_validated_json(json)?;
        let inner: SeasonInner = serde_json::from_value(inner)?;

        let mut recordings = Vec::new();

        if let Some(cache) = cache {
            for (rec_path, cache) in inner.recordings.iter().zip(cache.recordings.iter()) {
                let recording = Recording::load(&json_root.join(rec_path), ondisk_root, Some(cache))?;        
                recordings.push(recording);
            }
        } else {
            for rec_path in &inner.recordings {
                let recording = Recording::load(&json_root.join(rec_path), ondisk_root, None)?;
                recordings.push(recording);
            }
    
        }


        Ok(Season {
            title: inner.title,
            recordings,
            //ondisk_root: ondisk_root.to_owned(),
        })
    }
}

#[derive(Deserialize, Debug)]
pub(crate) struct RecordingInner {
    #[serde(rename = "$schema")]
    schema: String,

    pub title: String,
    pub data_folder: String,
    pub stereo_mix: TrackInner,
    pub recorded_date: String,
    pub youtube_url: Option<String>,
    pub torrent: Option<String>,
    pub bpm: Option<String>,
    pub tracks: Vec<TrackInner>,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Recording {
    pub title: String,
    pub data_folder: String,
    pub stereo_mix: Track,
    pub recorded_date: String,
    pub torrent: Option<String>,
    pub tracks: Vec<Track>,
    pub tags: Vec<String>,
    pub bpm: Option<String>,
    pub youtube_url: Option<String>,

    //ondisk_root: PathBuf,
}
impl Recording {
    /// Load info about a recording, given a path to its json file
    pub fn load<P: AsRef<Path>>(json: P, ondisk_root: Option<&Path>, cache: Option<&Recording>) -> Result<Self, anyhow::Error> {
        let json = json.as_ref();
        let _json_root = json.parent().unwrap();

        let inner = crate::get_validated_json(json)?;
        let inner: RecordingInner = serde_json::from_value(inner)?;

        let ondisk_root = ondisk_root.map(|p| p.join(&inner.data_folder));

        let tracks = if let Some(cache) = cache {
            // gotta find the corresponding track from the cache
            inner
            .tracks
            .into_iter()
            .map(|tr| {
                let tr_id = tr.id;
                Track::from_inner(tr, ondisk_root.as_deref(), cache.tracks.iter().find(|t| t.id == tr_id)).unwrap()
            })
            .collect()
        } else {
            inner
            .tracks
            .into_iter()
            .map(|tr| Track::from_inner(tr, ondisk_root.as_deref(), None).unwrap())
            .collect()
        };
        // let tracks = inner
        //     .tracks
        //     .into_iter()
        //     .map(|tr| Track::from_inner(tr, &ondisk_root).unwrap())
        //     .collect();

        let stereo_mix = Track::from_inner(inner.stereo_mix, ondisk_root.as_deref(), cache.as_ref().map(|c| &c.stereo_mix))?;

        Ok(Recording {
            title: inner.title,
            data_folder: inner.data_folder,
            stereo_mix,
            recorded_date: inner.recorded_date,
            youtube_url: inner.youtube_url,
            torrent: inner.torrent,
            bpm: inner.bpm,
            tracks,
            tags: inner.tags,
            //ondisk_root: ondisk_root.to_owned(),
        })
    }
    pub fn format_info(&self) -> String {
        let sample_rate: f32 = self.stereo_mix.media_info.sample_rate.parse().unwrap();

        format!(
            "{}ch {:.1}kHz {}bit",
            self.stereo_mix.media_info.channels,
            sample_rate / 1000.0,
            self.stereo_mix.media_info.bit_depth
        )
    }

    pub fn duration(&self) -> String {
        let sec: f32 = self.stereo_mix.media_info.duration.parse().unwrap();
        let sec = sec.floor() as u64;
        if sec <= 59 {
            format!("{}s", sec)
        } else {
            let min = (sec as f32 / 60.0).floor() as u64;
            let sec = sec - (min * 60);
            format!("{}m {}s", min, sec)
        }
    }

    pub fn flac_size_str(&self) -> String {
        let total_bytes = self
            .tracks
            .iter()
            .fold(self.stereo_mix.flac_size_bytes(), |v, t| v + t.flac_size_bytes());
        format!("{}MB", total_bytes / 1024 / 1024)
    }

    pub fn ogg_size_str(&self) -> String {
        let total_bytes = self
            .tracks
            .iter()
            .fold(self.stereo_mix.ogg_size_bytes(), |v, t| v + t.ogg_size_bytes());
        format!("{}MB", total_bytes / 1024 / 1024)
    }
}

/// This structure is loaded directly from the JSON files in the data directdory
#[derive(Deserialize, Debug)]
pub(crate) struct TrackInner {
    pub id: u8,
    pub name: String,
    pub flac: String,
    vorbis: String,
    mp3: Option<String>,
    pub patch_notes: Option<String>,
}

impl TrackInner {
    pub fn vorbis(&self) -> Cow<Path> {
        if self.vorbis.contains("{FLACBASE}") {
            let t = Path::new(&self.flac);
            let base = t.file_stem().expect("No filestem on flac").to_string_lossy();
            Cow::Owned(PathBuf::from(self.vorbis.replace("{FLACBASE}", &base)))
        } else {
            Cow::Borrowed(Path::new(&self.vorbis))
        }
    }
    pub fn mp3<'a>(&'a self) -> Option<Cow<'a, Path>> {
        match &self.mp3 {
            None => None,
            Some(mp3) if mp3.contains("{FLACBASE}") => {
                let t = Path::new(&self.flac);
                let base = t.file_stem().expect("No filestem on flac").to_string_lossy();
                Some(Cow::Owned(PathBuf::from(mp3.replace("{FLACBASE}", &base))))
            }
            Some(mp3) => {
                Some(Cow::Borrowed(Path::new(mp3.as_str())))
            }
        }
    }
}

/// This structure is used to save the metadata.json files
#[derive(Debug, Deserialize, Serialize)]
pub struct Track {
    pub id: u8,
    pub name: String,
    pub flac: String,
    pub vorbis: String,
    pub mp3: Option<String>,
    pub patch_notes: Option<String>,

    /// Folder on the current machine can this track be found
    ondisk_root: Option<PathBuf>,

    /// Technical info about this track
    pub media_info: MediaInfo,

    pub flac_bytes: u64,
    pub ogg_bytes: u64,
    pub mp3_bytes: u64,
}

impl Track {
    pub(crate) fn from_inner(inner: TrackInner, ondisk_root: Option<&Path>, cache: Option<&Track>) -> Result<Self, anyhow::Error> {
        let flac_bytes = ondisk_root
            .and_then(|p| std::fs::metadata(p.join(&inner.flac)).ok())
            .map(|md| md.len())
            .unwrap_or_else(|| cache.map(|c| c.flac_bytes).unwrap_or_else(|| panic!("Can't construct track for {:?}", inner)));

        let ogg_bytes = ondisk_root
            .and_then(|p| std::fs::metadata(p.join(&inner.vorbis())).ok())
            .map(|md| md.len())
            .unwrap_or_else(|| cache.map(|c| c.ogg_bytes).unwrap_or(0));

        let mp3_bytes = ondisk_root
            .and_then(|p| inner.mp3().and_then(|mp3| std::fs::metadata(p.join(mp3)).ok()))
            .map(|md| md.len())
            .unwrap_or_else(|| cache.map(|c| c.ogg_bytes).unwrap_or(0));

        let media_info: MediaInfo = ondisk_root
            .map(|p| MediaInfo::new(p.join(&inner.flac)).unwrap())
            .unwrap_or_else(|| cache.map(|c| c.media_info.clone()).unwrap());

        let flac_basename = {
            let t = Path::new(&inner.flac);
            t.file_stem().expect("no flac file stem").to_string_lossy().to_string()
        };

        Ok(Track {
            media_info,
            id: inner.id,
            name: inner.name,
            flac: inner.flac,
            vorbis: inner.vorbis.replace("{FLACBASE}", &flac_basename),
            mp3: inner.mp3.map(|mp3| mp3.replace("{FLACBASE}", &flac_basename)),
            patch_notes: inner.patch_notes,
            ondisk_root: ondisk_root.map(Path::to_owned),
            flac_bytes,
            ogg_bytes,
            mp3_bytes,
        })
    }

    pub fn flac_ondisk(&self) -> Option<PathBuf> {
        self.ondisk_root.as_ref().map(|p| p.join(&self.flac))
    }
    pub fn ogg_ondisk(&self) -> Option<PathBuf> {
        self.ondisk_root.as_ref().map(|p| p.join(&self.vorbis))
    }

    pub fn mp3_ondisk(&self) -> Option<PathBuf> {
        self.ondisk_root.as_ref().and_then(|p| self.mp3.as_ref().map(|mp3| p.join(&mp3)))
    }

    pub fn flac_size_str(&self) -> String {
        format!("{}MB", self.flac_bytes / 1024 / 1024)
    }

    pub fn flac_size_bytes(&self) -> u64 {
        self.flac_bytes
    }

    pub fn ogg_size_str(&self) -> String {
        format!("{}MB", self.ogg_bytes / 1024 / 1024)
    }

    pub fn ogg_size_bytes(&self) -> u64 {
        self.ogg_bytes
    }

    pub fn mp3_size_str(&self) -> String {
        format!("{}MB", self.mp3_bytes / 1024 / 1024)
    }

    pub fn mp3_size_bytes(&self) -> u64 {
        self.mp3_bytes
    }

    pub fn patch_notes(&self) -> &str {
        if let Some(s) = &self.patch_notes {
            s.as_ref()
        } else {
            ""
        }
    }
}
