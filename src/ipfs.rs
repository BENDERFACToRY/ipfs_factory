use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;

use std::convert::TryFrom;
use std::str::FromStr;
use std::{path::Path, process::Command};

#[derive(Serialize, Deserialize, Debug)]
struct IPFSHash {
    #[serde(rename = "Hash")]
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IPFSObject {
    #[serde(rename = "Links")]
    pub links: Vec<IPFSLink>,
    // #[serde(rename="Data")]
    // pub data: String,
    #[serde(skip)]
    hash: Option<cid::Cid>,
}

impl IPFSObject {
    pub fn get(hash: &cid::Cid) -> anyhow::Result<IPFSObject> {
        let output = Command::new("ipfs")
            .arg("object")
            .arg("get")
            .arg(format!("{}", hash))
            .arg("--encoding=json")
            .output()?;

        if !output.status.success() {
            bail!("Failed to run ipfs object patch: {}", output.status);
        }

        let mut ipfs_object: IPFSObject = serde_json::from_slice(&output.stdout)?;
        ipfs_object.hash = Some(hash.clone());

        Ok(ipfs_object)
    }

    pub fn cid(&self) -> &cid::Cid {
        self.hash.as_ref().unwrap()
    }

    pub fn add_link(&self, link_name: &str, link_hash: &cid::Cid) -> anyhow::Result<IPFSObject> {
        let output = Command::new("ipfs")
            .arg("object")
            .arg("patch")
            .arg("add-link")
            .arg(format!("{}", self.hash.as_ref().unwrap()))
            .arg(link_name)
            .arg(format!("{}", link_hash))
            .arg("--encoding=json")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to run ipfs object patch: {} {}", output.status, stderr);
        }
        let new_hash: IPFSHash = serde_json::from_slice(&output.stdout)?;

        let new_cid = cid::Cid::try_from(new_hash.hash.as_str())?;

        IPFSObject::get(&new_cid)
    }
}

fn ipfs_add<P: AsRef<Path>>(path: P) -> anyhow::Result<cid::Cid> {
    let output = Command::new("ipfs")
        .arg("add")
        .arg("--pin=false")
        .arg("-q")
        .arg(path.as_ref())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to run ipfs add: {} {}", output.status, stderr);
    }

    let new_hash = String::from_utf8_lossy(&output.stdout);
    let new_cid = cid::Cid::from_str(new_hash.trim())?;

    Ok(new_cid)
}

pub fn patch_root_object<P: AsRef<Path>>(root_hash: &cid::Cid, root_dir: P) -> anyhow::Result<cid::Cid> {
    let root_dir: &Path = root_dir.as_ref();
    let patchable = vec!["ToS.txt", "index.html", "style.css"];
    let mut root_obj = IPFSObject::get(root_hash)?;

    let links = root_obj.links.clone();

    for link in links {
        let local_link = root_dir.join(&link.name);

        if local_link.exists() && local_link.is_file() && patchable.contains(&link.name.as_str()) {
            let new_cid = ipfs_add(&local_link)?;
            println!("Patching {} with {} ({})", link.name, local_link.display(), new_cid);
            root_obj = root_obj.add_link(&link.name, &new_cid)?;
        }
        if local_link.exists() && local_link.is_dir() {
            let new_cid = patch_root_object(&link.hash, &local_link)?;
            root_obj = root_obj.add_link(&link.name, &new_cid)?;
        }
    }

    Ok(root_obj.cid().clone())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IPFSLink {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Hash", with = "serde_cid")]
    pub hash: cid::Cid,
    #[serde(rename = "Size")]
    pub size: usize,
}

mod serde_cid {
    use serde::Deserialize;
    use serde::{Deserializer, Serializer};
    use std::str::FromStr;

    pub fn serialize<S>(c: &cid::Cid, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", c);
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<cid::Cid, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        cid::Cid::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn cid() {
        let cid = cid::Cid::from_str("QmPkzy9kPR9U5V3bNdHix3DcfR86e2dNefnGMkX9CVo1Wh").unwrap();
        println!("{:?}", cid);
        println!("{}", cid);
    }

    #[test]
    fn object() {
        let cid = cid::Cid::from_str("QmPkzy9kPR9U5V3bNdHix3DcfR86e2dNefnGMkX9CVo1Wh").unwrap();
        let obj = IPFSObject::get(&cid).unwrap();
        // for link in obj.links {
        //     println!("{} {:?}", link.name, link.hash);
        // }
        let new = obj.add_link(
            "ToS.txt",
            &cid::Cid::from_str("QmXdCEDuqTgR2gfmVUyYCojvmxqRuQaL97RGNDjozrYCxE").unwrap(),
        )
        .unwrap();
        println!("{}", new.cid());
    }
}
