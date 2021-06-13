use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;

use std::{ffi::OsStr, convert::TryFrom};
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

fn ipfs_add<P: AsRef<Path>>(path: P, is_folder: bool) -> anyhow::Result<cid::Cid> {
    let mut cmd = Command::new("ipfs");
    cmd.arg("add")
        .arg("--pin=false")
        .arg("-Q")
        .arg(path.as_ref());
    if is_folder {
        cmd.arg("-r");
    }
    let output = cmd.output()?;

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
    // let patchable = vec!["ToS.txt", "index.html", "style.css", "metadata.json", "css", "webfonst"];
    let mut root_obj = IPFSObject::get(root_hash)?;

    for local_link in root_dir.read_dir()? {
        let local_link = local_link?;
        let local_link_path = local_link.path();

        // find the corresponding link in the IPFS structure (if it exists)
        let maybe_link = root_obj.links.iter().find(|l| local_link.file_name() == AsRef::<OsStr>::as_ref(&l.name));
        if let Some(ext) = local_link_path.extension() {
            if (ext == "ogg" || ext == "flac") && maybe_link.is_some() {
                // we don't patch ogg/flac audio files if they already exist in IPFS
                continue
            };
        }

        if local_link_path.is_file() {
            if let Some(link) = maybe_link {
                let new_cid = ipfs_add(&local_link_path, false)?;
                if new_cid != link.hash {
                    println!("Patching {} with {} ({})", link.name, local_link_path.display(), new_cid);
                    root_obj = root_obj.add_link(&link.name, &new_cid)?;
                }
            } else {
                let new_cid = ipfs_add(&local_link_path, true)?;
                let new_link_name = local_link.file_name();
                root_obj = root_obj.add_link(&new_link_name.to_string_lossy(), &new_cid)?;
                println!("Added new link to {:?} ({})", new_link_name, new_cid);
            }
        } else if local_link_path.is_dir() {
            if let Some(link) = maybe_link {
                // link already exists, so recurse
                let new_cid = patch_root_object(&link.hash, &local_link_path)?;
                if new_cid != link.hash {
                    root_obj = root_obj.add_link(&link.name, &new_cid)?;
                }
            } else {
                let new_cid = ipfs_add(&local_link_path, true)?;
                let new_link_name = local_link.file_name();
                root_obj = root_obj.add_link(&new_link_name.to_string_lossy(), &new_cid)?;
                println!("Added new link to {:?} ({})", new_link_name, new_cid);
            }
            
        }

    }

    // now look for links the the IPFS object that don't exist locally and print a warning about them
    for link in &root_obj.links {
        let maybe_local = root_dir.join(&link.name);
        if !maybe_local.exists() {
            println!("Warning: {} exists in IPFS, but not on the filesystem {:?}", link.name, maybe_local);
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

pub fn prime_public_gateways(root_hash: &cid::Cid) -> anyhow::Result<()> {
    let gateways = vec![
        "https://{base32}.ipfs.dweb.link",
        "https://gateway.ipfs.io/ipfs/{v0}",
        "https://ipfs.io/ipfs/{v0}",
        "https://ipfs.overpi.com/ipfs/{v0}",
        // "https://{base32}.ipfs.ipfs.stibarc.com",
        "https://{base32}.ipfs.cf-ipfs.com",
        "https://{base32}.ipfs.jacl.tech",
    ];

    let b32 = cid::Cid::new_v1(root_hash.codec(), root_hash.hash().to_owned());
    let v0 = cid::Cid::new_v0(root_hash.hash().to_owned())?;

    let ipfs_root = IPFSObject::get(&root_hash)?;

    for gw in gateways {
        let gw = gw
            .replace("{base32}", &format!("{}", b32))
            .replace("{v0}", &format!("{}", v0));
        let base_url = reqwest::Url::parse(&gw)?;
        print!("Priming {}... ", base_url);
        let resp = reqwest::blocking::get(base_url.clone())?;
        println!(" {}", resp.status());

        for link in &ipfs_root.links {
            let url = reqwest::Url::parse(&format!("{}/{}", gw, link.name))?;
            print!("  {}...", url);
            let resp = reqwest::blocking::get(url.clone())?;
            println!(" {}", resp.status());
        }
    }

    Ok(())
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
