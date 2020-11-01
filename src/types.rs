
use std::path::{Path, PathBuf};

use serde::Deserialize;

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
            recordings
        })

        
    }
}



#[derive(Deserialize, Debug)]
pub(crate) struct RecordingInner {
    #[serde(rename = "$schema")]
    schema: String,

    pub title: String,
    pub data_folder: String,
    pub stereo_mix: String,
    pub recorded_date: String,
    pub torrent: String,
    pub tracks: Vec<TrackInner>,
    pub tags: Vec<String>,
}


#[derive(Debug)]
pub struct Recording {
    pub title: String,
    pub data_folder: String,
    pub stereo_mix: String,
    pub recorded_date: String,
    pub torrent: String,
    pub tracks: Vec<Track>,
    pub tags: Vec<String>,

    ondisk_root: PathBuf,
}
impl Recording {
    /// Load info about a recording, given a path to its json file
    pub fn load<P: AsRef<Path>>(json: P, ondisk_root: &Path) -> Result<Self, anyhow::Error> {


        let json = json.as_ref();
        let json_root = json.parent().unwrap();


        let inner = crate::get_validated_json(json)?;
        let inner: RecordingInner = serde_json::from_value(inner)?;

        let ondisk_root = ondisk_root.join(&inner.data_folder);

        
        let tracks = inner.tracks.into_iter().map(|tr| {
            Track::from_inner(tr, &ondisk_root)
        }).collect();

        Ok(Recording {
            title: inner.title,
            data_folder: inner.data_folder,
            stereo_mix: inner.stereo_mix,
            recorded_date: inner.recorded_date,
            torrent: inner.torrent,
            tracks,
            tags: inner.tags,
            ondisk_root: ondisk_root.to_owned(),
        })

        
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

    ondisk_root: PathBuf,
}

impl Track {
    pub(crate) fn from_inner(inner: TrackInner, ondisk_root: &Path) -> Self {
        Track {
            id: inner.id,
            name: inner.name,
            flac: inner.flac,
            vorbis: inner.vorbis,
            patch_notes: inner.patch_notes,
            ondisk_root: ondisk_root.to_owned(),
        }
    }

    pub fn flac_size(&self) -> String {
        if let Ok(md) = std::fs::metadata(self.ondisk_root.join(&self.flac)) {
            format!("{}MB", md.len() / 1024 / 1024)

        } else {
            format!("unknown")
        }
    }

    pub fn ogg_size(&self) -> String {
        if let Ok(md) = std::fs::metadata(self.ondisk_root.join(&self.vorbis)) {
            format!("{}MB", md.len() / 1024 / 1024)

        } else {
            format!("unknown")
        }
    }

    pub fn patch_notes(&self) -> &str {
        if let Some(s) = &self.patch_notes {
            s.as_ref()
        } else {
            ""
        }
    }
}

