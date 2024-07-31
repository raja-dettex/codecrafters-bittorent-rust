use std::collections::HashSet;

use crate::{peers::Peer, torrent::Torrent, tracker::TrackerResponse};

#[derive(Debug, PartialEq, Eq)]
pub struct PieceInfo  {
    peers : HashSet<usize>,
    piece_i : usize,
    length : usize,
    hash : [u8; 20], 
    seed : u64
}

impl Ord for PieceInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.peers.len()
            .cmp(&other.peers.len())
            .then(self.seed.cmp(&other.seed))
            .then(self.hash.cmp(&other.hash))
            .then(self.length.cmp(&other.length))
            .then(self.peers.iter().cmp(other.peers.iter()))
            .then(self.piece_i.cmp(&other.piece_i))
    }
}

impl PartialOrd for PieceInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PieceInfo { 
    pub(crate) fn new(piece_i : usize, t : &Torrent, peers : &[Peer]) -> Self {
        let piece_size = if piece_i == t.info.pieces.0.len() - 1 {
            let md = t.length() % t.info.plength;
            if md == 0 {
                t.info.plength
            } else {
                md
            }
        } else {
            t.info.plength
        };
        let hash = t.clone().info_hash();
        let peers = peers.iter().enumerate().filter_map(|(peer_i, peer) | { 
            peer.has_piece(piece_i).then_some(peer_i)
        }).collect();
        Self {
            peers : peers,
            piece_i: piece_i,
            length: piece_size,
            hash : hash,
            seed: 5 as u64
        }       
    }
    pub(crate) fn length(&self) -> usize { 
        self.length
    }
    pub(crate) fn peers(&self) -> &HashSet<usize> { 
        &self.peers
    }

    pub(crate) fn index(&self ) -> usize { 
        self.piece_i
    }
    pub(crate) fn hash(&self) -> [u8;20] { 
        self.hash
    }
}