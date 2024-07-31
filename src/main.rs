
use clap::{Parser, Subcommand};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::{fs::{OpenOptions, Permissions}, io::Write, net::SocketAddrV4, path::PathBuf};
use bittorrent_starter_rust::{peers::{Message, MessageFramer, MessageTag, PeerHandShake, Piece, Request}, torrent::{Keys, Torrent}, tracker::{urlencode, TrackerRequest, TrackerResponse}, BLOCK_MAX};
use anyhow::Context;
use hex;
use futures_util::stream::StreamExt;
use futures_util::sink::SinkExt;
use sha1::{Digest, Sha1};




#[allow(unused_variables, unused_imports)]




#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Args {

    #[command(subcommand)] 
    command : Command
}

#[derive(Subcommand)]
#[clap(rename_all = "snake_case" )]
enum Command { 
    Decode { value : String}, 
    Info { torrent : PathBuf},
    Peers { torrent : PathBuf},
    Handshake { torrent: PathBuf , peer : String},
    DownloadPiece { 
        #[arg(short, long)]
        output : PathBuf,
        torrent : PathBuf,
        piece : usize
    },
    Download { 
        #[arg(short, long)]
        output: PathBuf,
        torrent : PathBuf
    }
}




#[tokio::main]
async fn main() -> anyhow::Result<()> {

    let arg = Args::parse();
    match arg.command { 
        Command::Decode{.. } => { 
            unimplemented!("serde_bencode -> serde_json::Value to issues head");
        },
        Command::Info { torrent } =>  { 
            let mut f = std::fs::read(torrent).context("read torren file bytes")?;
            let tf_info: Torrent = serde_bencode::from_bytes(&mut f).context("parse the file")?;
            eprintln!("torrent file info : {:?} ", tf_info);
            println!("Tracker URL: {}", tf_info.announce);
            let length = if let Keys::SingleFile { length } = tf_info.info.keys { 
                length
            } else { 
                todo!()
            };
            println!("Length: {}", length);
            let info_hash = tf_info.clone().info_hash();
            println!("Info Hash: {}", hex::encode(info_hash));
            println!("Piece Length: {}", tf_info.info.plength);
            for hash in &tf_info.info.pieces.0  {
                println!("{}", hex::encode(&hash));
            }
            
        },
        Command::Peers{torrent} => {
            let file_bytes = std::fs::read(torrent).context("parse the file")?;
            let tf_info: Torrent = serde_bencode::from_bytes(&file_bytes).context("parse file bytes to structure file info")?;
            let length = if let Keys::SingleFile { length } = tf_info.info.keys { 
                length
            } else { 
                todo!()
            };
            let info_hash = tf_info.clone().info_hash();
            let request = TrackerRequest {  
                peer_id : "00112233445566778899".to_string(),
                port : 6881,
                uploaded: 0, 
                downloaded : 0,
                left : length, 
                compact : 1
            };
            let query_params = serde_urlencoded::to_string(&request).expect("encode into url params");
            let tracker_url = format!("{}?{}&info_hash={}", tf_info.announce, query_params, &urlencode(&info_hash));
            let res = reqwest::get(tracker_url).await?;
            let res_bytes = res.bytes().await.expect("expected response bytes");
            let response : TrackerResponse = serde_bencode::from_bytes(&res_bytes).expect("Tracker Response");
            for peer in response.peers.0 { 
                println!("{peer:?}");
            }
        }, 
        Command::Handshake { torrent , peer } =>  { 
            let mut f = std::fs::read(torrent).context("read torren file bytes")?;
            let tf_info: Torrent = serde_bencode::from_bytes(&mut f).context("parse the file")?;
            let info_hash = tf_info.clone().info_hash();
            
            let peer = peer.parse::<SocketAddrV4>().context("parse from the string")?;
            let mut peer_conn = tokio::net::TcpStream::connect(peer).await.context("connect to peer")?;
            let mut handshake = PeerHandShake::new(&info_hash, b"00112233445566778899");
            {
                let handshake_bytes = &mut handshake as *mut PeerHandShake as *mut [u8; std::mem::size_of::<PeerHandShake>()];
                let handshake_bytes: &mut [u8; std::mem::size_of::<PeerHandShake>()] = unsafe { &mut *handshake_bytes};
                peer_conn.write_all( handshake_bytes).await.context("write to conn")?;
                peer_conn.read_exact(handshake_bytes).await.context("read from other side of handshake")?;
            }
            println!("Peer ID : {}", hex::encode(&handshake.peer_id))
        },
        Command::DownloadPiece { output,torrent , piece: piece_i } =>  { 
            let mut f = std::fs::read(torrent).context("read torren file bytes")?;
            let tf_info: Torrent = serde_bencode::from_bytes(&mut f).context("parse the file")?;
            let info_hash = tf_info.clone().info_hash();
            let length = if let Keys::SingleFile { length } = tf_info.info.keys { 
                length
            } else { 
                todo!()
            };
            let request = TrackerRequest {  
                peer_id : "00112233445566778899".to_string(),
                port : 6881,
                uploaded: 0, 
                downloaded : 0,
                left : length, 
                compact : 1
            };
            let query_params = serde_urlencoded::to_string(&request).expect("encode into url params");
            let tracker_url = format!("{}?{}&info_hash={}", tf_info.announce, query_params, &urlencode(&info_hash));
            let res = reqwest::get(tracker_url).await?;
            let res_bytes = res.bytes().await.expect("expected response bytes");
            let tracker_response : TrackerResponse = serde_bencode::from_bytes(&res_bytes).expect("Tracker Response");
            eprintln!("{:?}", tracker_response.peers.0);
            let peer = tracker_response.peers.0[1];
            let mut peer_conn = tokio::net::TcpStream::connect(peer).await.context("connect to peer")?;
            let mut handshake = PeerHandShake::new(&info_hash, b"00112233445566778899");
            {
                let  handshake_bytes = handshake.as_bytes_mut();
                peer_conn.write_all( handshake_bytes).await.context("write to conn")?;
                peer_conn.read_exact(handshake_bytes).await.context("read from other side of handshake")?;
            }
            
            let mut framed = tokio_util::codec::Framed::new(peer_conn, MessageFramer);
            let bitfield  = framed.next().await.context("first message tag should be a bitfield tag")??;
            assert_eq!(bitfield.tag, MessageTag::Bitfield);
            framed.send(Message { 
                tag : MessageTag::Interested,
                payload : Vec::new()
            }).await.context("send interested message")?;
            let unchoked = framed.next().await.context("should receive unchoked message")?.expect("expected unchoked");
            assert_eq!(unchoked.tag, MessageTag::Unchoke);
            let piece_hash = tf_info.info.pieces.0[piece_i];
            let piece_size = if piece_i == tf_info.info.pieces.0.len() - 1 {
                let md = length % tf_info.info.plength;
                if md == 0 {
                    tf_info.info.plength
                } else {
                    md
                }
            } else {
                tf_info.info.plength
            };
            assert!(piece_i < tf_info.info.pieces.0.len());
            let n_blocks = (piece_size + (BLOCK_MAX - 1))/BLOCK_MAX;
            let mut all_blocks: Vec<u8> = Vec::with_capacity(piece_size);
            for block in 0..n_blocks { 
                let block_size = if block == n_blocks - 1 { 
                    let md = piece_size % BLOCK_MAX;
                    if md == 0 {
                        BLOCK_MAX
                    } else {
                        md
                    }
                } else { 
                    BLOCK_MAX
                };
                let mut request = Request::new(piece_i as u32, (block * BLOCK_MAX) as u32, block_size as u32 );
                let request_bytes = Vec::from(request.as_bytes_mut());
                framed.send(Message { tag: MessageTag::Request, payload: request_bytes }).await
                .with_context(||format!("download request for each block {block}" ))?;
                let piece_rcvd = framed.next().await.expect("aways sends a piece").context("invalid message")?;
                assert_eq!(piece_rcvd.tag , MessageTag::Piece); 
                assert!(!piece_rcvd.payload.is_empty());
                let piece = Piece::ref_from_bytes(&piece_rcvd.payload[..]).expect("expecte a piece");
                assert_eq!(piece.index() as usize, piece_i);
                assert_eq!(piece.begin() as usize, block * BLOCK_MAX);
                assert_eq!(piece.block().len(), block_size);
                // assert_eq!(piece.block().len(), block_size);
                all_blocks.extend(piece.block());
            } 
            assert_eq!(all_blocks.len(), piece_size);
            let mut hasher = Sha1::new();
            hasher.update(&all_blocks);
            let hash : [u8; 20] = hasher.finalize().try_into().expect("hash the block");
            assert_eq!(hash, piece_hash);
            //std::fs::create_dir_all(&output).expect("msg");
            
            let mut file = tokio::fs::OpenOptions::new().write(true).create(true).open(&output).await.expect("open file pointer");
            let n = file.write(&all_blocks).await.expect("write");
            println!("written {n} bytes");
            //(&mut std::fs::OpenOptions::new().write(true).create(true).open(output).expect("here")).write_all(&all_blocks).await.expect("panicked to write");       
            
        },
        Command::Download { output, torrent } => {
            let torrent = Torrent::read(torrent).await?;
            torrent.print_tree();
            let files = torrent.download_all().await?;
            let mut fp = tokio::fs::OpenOptions::new().write(true).create(true).open(output).await?;
            fp.write(files.into_iter().next().expect("").bytes()).await?;
        }
    }
    Ok(())
    
}


