use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::MediaInfo;

#[derive(Deserialize, Debug)]
/// This is the raw JSON struct
pub(crate) struct SeasonInner {
    #[serde(rename = "$schema")]
    schema: String,
    pub title: String,
    pub recordings: Vec<String>,
}

pub struct Season {
    pub title: String,
    pub recordings: Vec<Recording>,

    pub(crate) ondisk_root: PathBuf,
}

impl Season {
    pub fn load<P: AsRef<Path>>(json: P, ondisk_root: &Path) -> Result<Self, anyhow::Error> {
        let json = json.as_ref();
        let json_root = json.parent().unwrap();

        let inner = crate::get_validated_json(json)?;
        let inner: SeasonInner = serde_json::from_value(inner)?;

        let mut recordings = Vec::new();

        for rec_path in &inner.recordings {
            let recording = Recording::load(&json_root.join(rec_path), ondisk_root)?;

            // let recording = crate::get_validated_json()?;
            // let recording: Recording = serde_json::from_value(recording)?;

            recordings.push(recording);
        }

        Ok(Season {
            title: inner.title,
            recordings,
            ondisk_root: ondisk_root.to_owned(),
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
    pub torrent: String,
    pub bpm: Option<u8>,
    pub tracks: Vec<TrackInner>,
    pub tags: Vec<String>,
}

#[derive(Debug)]
pub struct Recording {
    pub title: String,
    pub data_folder: String,
    pub stereo_mix: Track,
    pub recorded_date: String,
    pub torrent: String,
    pub tracks: Vec<Track>,
    pub tags: Vec<String>,
    pub bpm: Option<u8>,
    pub youtube_url: Option<String>,

    ondisk_root: PathBuf,
}
impl Recording {
    /// Load info about a recording, given a path to its json file
    pub fn load<P: AsRef<Path>>(json: P, ondisk_root: &Path) -> Result<Self, anyhow::Error> {
        let json = json.as_ref();
        let _json_root = json.parent().unwrap();

        let inner = crate::get_validated_json(json)?;
        let inner: RecordingInner = serde_json::from_value(inner)?;

        let ondisk_root = ondisk_root.join(&inner.data_folder);

        let tracks = inner
            .tracks
            .into_iter()
            .map(|tr| Track::from_inner(tr, &ondisk_root).unwrap())
            .collect();

        Ok(Recording {
            title: inner.title,
            data_folder: inner.data_folder,
            stereo_mix: Track::from_inner(inner.stereo_mix, &ondisk_root)?,
            recorded_date: inner.recorded_date,
            youtube_url: inner.youtube_url,
            torrent: inner.torrent,
            bpm: inner.bpm,
            tracks,
            tags: inner.tags,
            ondisk_root: ondisk_root.to_owned(),
        })
    }
    pub fn format_info(&self) -> String {
        let sample_rate: f32 = self.tracks[0].media_info.sample_rate.parse().unwrap();

        format!(
            "{}ch {:.1}kHz {}bit",
            self.tracks[0].media_info.channels,
            sample_rate / 1000.0,
            self.tracks[0].media_info.bit_depth
        )
    }

    pub fn duration(&self) -> String {
        let sec: f32 = self.tracks[0].media_info.duration.parse().unwrap();
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

#[derive(Deserialize, Debug)]
pub(crate) struct TrackInner {
    pub id: u8,
    pub name: String,
    pub flac: String,
    pub vorbis: String,
    pub patch_notes: Option<String>,
}

#[derive(Debug)]
pub struct Track {
    pub id: u8,
    pub name: String,
    pub flac: String,
    pub vorbis: String,
    pub patch_notes: Option<String>,

    /// Folder on the current machine can this track be found
    ondisk_root: PathBuf,

    /// Technical info about this track
    media_info: MediaInfo,
}

impl Track {
    pub(crate) fn from_inner(inner: TrackInner, ondisk_root: &Path) -> Result<Self, anyhow::Error> {
        Ok(Track {
            media_info: MediaInfo::new(ondisk_root.join(&inner.flac))?,
            id: inner.id,
            name: inner.name,
            flac: inner.flac,
            vorbis: inner.vorbis,
            patch_notes: inner.patch_notes,
            ondisk_root: ondisk_root.to_owned(),
        })
    }

    pub fn flac_ondisk(&self) -> PathBuf {
        self.ondisk_root.join(&self.flac)
    }
    pub fn ogg_ondisk(&self) -> PathBuf {
        self.ondisk_root.join(&self.vorbis)
    }

    pub fn flac_size_str(&self) -> String {
        if let Ok(md) = std::fs::metadata(self.ondisk_root.join(&self.flac)) {
            format!("{}MB", md.len() / 1024 / 1024)
        } else {
            format!("unknown")
        }
    }

    pub fn flac_size_bytes(&self) -> u64 {
        std::fs::metadata(self.ondisk_root.join(&self.flac)).unwrap().len()
    }

    pub fn ogg_size_str(&self) -> String {
        if let Ok(md) = std::fs::metadata(self.ondisk_root.join(&self.vorbis)) {
            format!("{}MB", md.len() / 1024 / 1024)
        } else {
            format!("unknown")
        }
    }

    pub fn ogg_size_bytes(&self) -> u64 {
        std::fs::metadata(self.ondisk_root.join(&self.vorbis)).unwrap().len()
    }

    pub fn patch_notes(&self) -> &str {
        if let Some(s) = &self.patch_notes {
            s.as_ref()
        } else {
            ""
        }
    }
}
