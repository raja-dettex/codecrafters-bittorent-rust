use std::path::Path;

use anyhow::{Context, Ok};
use serde::*;
use super::hash::Hashes;
use sha1::{Sha1, Digest};
use super::download;
#[derive( Clone, Serialize, Deserialize, Debug)]
pub struct Torrent { 
    // URL to a "tracker", which is a central server that keeps track of peers participating in the sharing of a torrent 
    pub announce : String, 
    // A dictionary with keys
    pub info : Info
}

impl Torrent { 
    pub fn info_hash(self) -> [u8; 20] { 
        let info_encoded = serde_bencode::to_bytes(&self.info).expect("get bytes from info type");
        let mut hasher = Sha1::new();
        hasher.update(info_encoded);
        let info_hash = hasher.finalize();
        info_hash.try_into().expect("GenericArray<u8> to [u8;20]")
    }

    pub async fn read(file : impl AsRef<Path>) -> anyhow::Result<Self>  {
        let mut f = tokio::fs::read(file).await.context("read torren file bytes")?;
        let tf_info: Torrent = serde_bencode::from_bytes(&mut f).context("parse the file")?;
        Ok(tf_info)
    }
    pub fn length(&self) -> usize { 
        match &self.info.keys {
            Keys::SingleFile { length } => *length,
            Keys::MultiFile { files } => files.iter().map(|file| file.length).sum(),
        }
    }
    pub fn print_tree(&self)  { 
        match &self.info.keys {
            Keys::SingleFile { .. } => eprintln!("{}", self.info.name),
            Keys::MultiFile { files } => { 
                for file in files { 
                    eprintln!("{:?}", file.path.join(std::path::MAIN_SEPARATOR_STR));
                }
            },
        }
    }
    pub async fn download_all(&self) -> anyhow::Result<download::Downloaded> { 
        download::all(&self).await
    }
}


#[derive( Serialize, Deserialize, Clone, Debug)]
pub struct Info { 

    // suggested name  of the torrent file
    pub name : String,

    // size of the file in bytes, for single-file torrents
    #[serde(rename="piece length")] 
    pub plength : usize,

    // pieces maps to a string whose length is a multiple of 20. It is to be subdivided into strings of length 20, 
    // each of which is the SHA1 hash of the piece at the corresponding index
    pub pieces : Hashes,

     // There is also a key length or a key files, but not both or neither. 
     // If length is present then the download represents a single file, otherwise it represents a set of files which go in a directory structure.
    #[serde(flatten)]
    pub keys: Keys
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Keys { 
    SingleFile { 
        length : usize
    }, 
    MultiFile { 
        files  : Vec<FileInfo>
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileInfo { 
    // length - The length of the file, in bytes.
    pub length : usize,
    // path - A list of UTF-8 encoded strings corresponding to subdirectory names, 
    // the last of which is the actual file name (a zero length list is an error case). 
    pub path : Vec<String> 

}
