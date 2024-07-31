
use std::{collections::BinaryHeap, vec};

use futures_util::StreamExt;
use sha1::{Sha1, Digest};
use tokio::task::JoinSet;
use anyhow::Context;

use crate::{peers::{MessageTag, Peer, Piece}, piece::{self, PieceInfo}, torrent::{FileInfo, Keys, Torrent}, tracker::TrackerResponse};
use crate::BLOCK_MAX;

pub(crate) async fn all(t: &Torrent) -> anyhow::Result<Downloaded> {
    let info_hash = t.clone().info_hash();
    let peer_info = TrackerResponse::query_tracker_info(t, info_hash)
        .await
        .context("query tracker for peer info")?;

    let mut peer_list = Vec::new();
    let mut peers = futures_util::stream::iter(peer_info.peers.0.iter())
        .map(|&peer_addr| async move {
            let peer = Peer::new(peer_addr, info_hash).await;
            (peer_addr, peer)
        })
        .buffer_unordered(5 /* user config */);
    while let Some((peer_addr, peer)) = peers.next().await {
        match peer {
            Ok(peer) => {
                peer_list.push(peer);
                if peer_list.len() >= 5
                /* TODO: user config */
                {
                    break;
                }
            }
            Err(e) => {
                eprintln!("failed to connect to peer {peer_addr:?}: {e:?}");
            }
        }
    }
    drop(peers);
    let mut peers = peer_list;

    let mut need_pieces = BinaryHeap::new();
    let mut no_peers = Vec::new();
    for piece_i in 0..t.info.pieces.0.len() {
        let piece = PieceInfo::new(piece_i, &t, &peers);
        if piece.peers().is_empty() {
            no_peers.push(piece);
        } else {
            need_pieces.push(piece);
        }
    }

    // TODO
    //assert!(no_peers.is_empty());

    let mut all_pieces = vec![0; t.length()];
    while let Some(piece) = need_pieces.pop() {
        // the + (BLOCK_MAX - 1) rounds up
        let piece_size = piece.length();
        let nblocks = (piece_size + (BLOCK_MAX - 1)) / BLOCK_MAX;
        let peers: Vec<_> = peers
            .iter_mut()
            .enumerate()
            .filter_map(|(peer_i, peer)| piece.peers().contains(&peer_i).then_some(peer))
            .collect();

        let (submit, tasks) = kanal::bounded_async(nblocks);
        for block in 0..nblocks {
            submit
                .send(block)
                .await
                .expect("bound holds all these items");
        }
        let (finish, mut done) = tokio::sync::mpsc::channel(nblocks);
        let mut participants = futures_util::stream::futures_unordered::FuturesUnordered::new();
        for peer in peers {
            participants.push(peer.participate(
                piece.index(),
                piece_size,
                nblocks,
                submit.clone(),
                tasks.clone(),
                finish.clone(),
            ));
        }
        drop(submit);
        drop(finish);
        drop(tasks);

        let mut all_blocks = vec![0u8; piece_size];
        let mut bytes_received = 0;
        loop {
            tokio::select! {
                joined = participants.next(), if !participants.is_empty() => {
                    // if a participant ends early, it's either slow or failed
                    match joined {
                        None => {
                            // there are no peers!
                            // this must mean we are about to get None from done.recv(),
                            // so we'll handle it there
                        }
                        Some(Ok(_)) => {
                            // the peer gave up because it timed out
                            // nothing to do, except maybe de-prioritize this peer for later
                            // TODO
                        }
                        Some(Err(_)) => {
                            // the peer failed and should be removed
                            // it already isn't participating in this piece any more, so this is
                            // more of an indicator that we shouldn't try this peer again, and
                            // should remove it from the global peer list
                            // TODO
                        }
                    }
                }
                piece = done.recv() => {
                    if let Some(piece) = piece {
                        // keep track of the bytes in message
                        let piece = crate::peers::Piece::ref_from_bytes(&piece.payload[..])
                            .expect("always get all Piece response fields from peer");
                        bytes_received += piece.block().len();
                        all_blocks[piece.begin() as usize..].copy_from_slice(piece.block());
                    } else {
                        // have received every piece (or no peers left)
                        // this must mean that all participations have either exited or are waiting
                        // for more work -- in either case, it is okay to drop all the participant
                        // futures.
                        break;
                    }
                }
            }
        }
        drop(participants);

        if bytes_received == piece_size {
            // great, we got all the bytes
        } else {
            // we'll need to connect to more peers, and make sure that those additional peers also
            // have this piece, and then download the pieces we _didn't_ get from them.
            // probably also stick this back onto the pieces_heap.
            anyhow::bail!("no peers left to get piece {}", piece.index());
        }

        let mut hasher = Sha1::new();
        hasher.update(&all_blocks);
        let hash: [u8; 20] = hasher
            .finalize()
            .try_into()
            .expect("GenericArray<_, 20> == [_; 20]");
        assert_eq!(hash, piece.hash());

        all_pieces[piece.index() * t.info.plength..].copy_from_slice(&all_blocks);
    }

    Ok(Downloaded {
        bytes: all_pieces,
        files: match &t.info.keys {
            Keys::SingleFile { length } => vec![FileInfo {
                length: *length,
                path: vec![t.info.name.clone()],
            }],
            Keys::MultiFile { files } => files.clone(),
        },
    })
}
pub struct Downloaded { 
    bytes : Vec<u8>,
    files : Vec<FileInfo>
}

impl<'a> IntoIterator for &'a Downloaded {
    type Item = DownloadedFile<'a>;

    type IntoIter = DownloadIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DownloadIter::new(self)
    }
}


pub struct DownloadIter<'a> { 
    downloaded : &'a Downloaded,
    file_iter : std::slice::Iter<'a, FileInfo>,
    offset : usize
}


impl<'a> DownloadIter<'a> { 
    pub fn new(downloaded: &'a Downloaded, ) -> Self { 
        Self{downloaded, file_iter : downloaded.files.iter(), offset: 0}
    }
}

impl<'a> Iterator for DownloadIter<'a> {
    type Item = DownloadedFile<'a>; 

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_iter.next()?;
        let bytes = &self.downloaded.bytes[self.offset..][..file.length];
        Some(DownloadedFile { file , bytes})
    }
}

pub struct DownloadedFile<'a> { 
    file : &'a FileInfo,
    bytes : &'a [u8]
}

impl<'a> DownloadedFile<'a> { 
    pub fn path(&self) -> &'a [String] {
        &self.file.path
    }

    pub fn bytes(&self ) -> &'a [u8] { 
        &self.bytes
    }
}