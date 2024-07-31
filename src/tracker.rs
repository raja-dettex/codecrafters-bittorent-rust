use anyhow::Ok;
use serde::{Deserialize, Serialize};

use crate::{peers::Peers, torrent::Torrent};


#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackerRequest { 
    //the info hash of the torrent
    // 20 bytes long, will need to be URL encoded
    // Note: this is NOT the hexadecimal representation, which is 40 bytes long

    // a unique identifier for your client
    // A string of length 20 that you get to pick. You can use something like 00112233445566778899.

    pub peer_id: String,
    // the port your client is listening on
    // You can set this to 6881, you will not have to support this functionality during this challenge.

    pub port: u16, 
    //the total amount uploaded so far
    // Since your client hasn't uploaded anything yet, you can set this to 0.
    pub uploaded: usize,
    pub downloaded: usize, // the total amount downloaded so far
    pub left: usize, // the number of bytes left to download
    pub compact: usize, // whether the peer list should use the compact representation
    
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse { 
    pub interval: usize, 
    // An integer, indicating how often your client should make a request to the tracker.
    // You can ignore this value for the purposes of this challenge.
    pub peers : Peers
    // A string, which contains list of peers that your client can connect to.
    // Each peer is represented using 6 bytes. The first 4 bytes are the peer's IP address and the last 2 bytes are the peer's port number.
}


impl TrackerResponse { 
    pub(crate) async fn query_tracker_info(t : &Torrent, info_hash : [u8; 20])  -> anyhow::Result<Self> {
         
        let request = TrackerRequest {  
            peer_id : "00112233445566778899".to_string(),
            port : 6881,
            uploaded: 0, 
            downloaded : 0,
            left : t.clone().length(), 
            compact : 1
        };
        
        let query_params = serde_urlencoded::to_string(&request).expect("encode into url params");
        let tracker_url = format!("{}?{}&info_hash={}", t.announce, query_params, &urlencode(&info_hash));
        let res = reqwest::get(tracker_url).await?;
        let res_bytes = res.bytes().await.expect("expected response bytes");
        let tracker_response : TrackerResponse = serde_bencode::from_bytes(&res_bytes).expect("Tracker Response");
        Ok(tracker_response)
    }
}

 
pub fn urlencode(t : &[u8;20]) -> String { 
    let mut encoded = String::with_capacity(3 * t.len());
    for byte in t { 
        encoded.push('%');
        encoded.push_str(&hex::encode(&[*byte]));
    }
    encoded
}