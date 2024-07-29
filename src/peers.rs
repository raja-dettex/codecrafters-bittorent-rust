use serde::de::{Visitor, Deserialize, Deserializer};
use serde::{de, Serialize};
use core::panic;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};
use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
pub struct PeersVisitor;
#[derive(Clone, Debug)]
pub struct Peers(pub Vec<SocketAddrV4>);


impl Serialize for Peers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut single_slice = Vec::with_capacity(6);
        for peer in &self.0 { 
            single_slice.extend(peer.ip().octets());
            single_slice.extend(peer.port().to_be_bytes());
        }
        serializer.serialize_bytes(&single_slice)
    }
}

impl<'de> Visitor<'de> for PeersVisitor {
    type Value = Peers;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a byte string whose whose length is multiple of 6")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error, {
        if v.len() % 6 != 0 { 
            return Err(E::custom(format!("length is {}", v.len())));
        }
        let slices  = v.chunks_exact(6).map(|slice_6| {
            SocketAddrV4::new(
                Ipv4Addr::new(slice_6[0], slice_6[1], slice_6[2], slice_6[3]), 
                u16::from_be_bytes([slice_6[4], slice_6[5]])
            )
        }).collect();
        Ok(Peers(slices))
        
    }
}

impl<'de> Deserialize<'de> for Peers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {
        deserializer.deserialize_bytes(PeersVisitor)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PeerHandShake { 
    pub length: u8, 
    pub bittorrent: [u8; 19],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20], 
    pub peer_id : [u8; 20]
}

impl PeerHandShake {
    pub fn new(info_hash : &[u8; 20], peer_id: &[u8; 20]) -> Self { 
        Self { length : 19, bittorrent : *b"BitTorrent protocol", reserved : [0; 8], info_hash : *info_hash, peer_id : *peer_id}
    }
    pub fn as_bytes_mut(&mut self) -> &mut [u8] { 
        let h_bytes = self as *mut Self as *mut [u8; std::mem::size_of::<Self>()];
        let h_bytes: &mut [u8; std::mem::size_of::<Self>()] = unsafe { &mut *h_bytes};
        h_bytes
        
    }
}
#[repr(C)]
pub struct Request { 
    index : [u8; 4],
    begin : [u8; 4], 
    length : [u8; 4]
}


impl Request { 
    pub fn new(index: u32, begin : u32, length : u32) -> Self { 
        Self { index : index.to_be_bytes(), begin : begin.to_be_bytes(), length: length.to_be_bytes()}
    }
    pub fn index(&self) -> u32 { 
        u32::from_be_bytes(self.index)
    }
    pub fn begin(&self) -> u32 { 
        u32::from_be_bytes(self.begin)
    }
    pub fn length(&self) -> u32 { 
        u32::from_be_bytes(self.length)
    }
    pub fn as_bytes_mut(&mut self) -> &mut [u8] { 
        let p_bytes = self as *mut Self as *mut [u8; std::mem::size_of::<Self>()];
        let p_bytes : &mut [u8; std::mem::size_of::<Self>()] = unsafe { &mut *p_bytes};
        p_bytes
    }
}
#[repr(C)]
pub struct Piece<T: ?Sized = [u8]> { 
    index : [u8; 4],
    begin : [u8; 4], 
    block : T
}


impl Piece { 
    
    pub fn index(&self) -> u32 { 
        u32::from_be_bytes(self.index)
    }
    pub fn begin(&self) -> u32 { 
        u32::from_be_bytes(self.begin)
    }
    pub fn block(&self) -> &[u8] { 
        &self.block
    }
    
    const PIECE_LEAD : usize = std::mem::size_of::<Piece<()>>();
    pub fn ref_from_bytes(data : &[u8]) -> Option<&Self> {
        if data.len() < Self::PIECE_LEAD {
            return None;
        }
        let n = data.len();
        let piece = &data[..n - Self::PIECE_LEAD] as *const [u8] as *const Piece;
        Some(unsafe { &*piece})
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
 pub enum MessageTag { 
    Choke = 0, 
    Unchoke = 1, 
    Interested = 2, 
    NotInterested = 3, 
    Have = 4,
    Bitfield = 5, 
    Request = 6, 
    Piece = 7, 
    Cancel = 8
}

#[derive(Debug)]
pub struct Message { 
    pub tag : MessageTag,
    pub payload : Vec<u8>
}


const MAX : usize = 1 << 16;

pub struct MessageFramer;

impl Decoder for MessageFramer {
    type Item = Message;
    type Error = std::io::Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            // Not enough data to read length marker.
            return Ok(None);
        }
        // Read length marker.
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;
        if length == 0 {
            // this is a heartbeat message.
            // discard it.
            src.advance(4);
            // and then try again in case the buffer has more messages
            return self.decode(src);
        }
        if src.len() < 5 {
            // Not enough data to read tag marker.
            return Ok(None);
        }
        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if length > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }
        if src.len() < 4 + length {
            // The full string has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(4 + length - src.len());
            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }
        // Use advance to modify src such that it no longer contains
        // this frame.
        let tag = match src[4] {
            0 => MessageTag::Choke,
            1 => MessageTag::Unchoke,
            2 => MessageTag::Interested,
            3 => MessageTag::NotInterested,
            4 => MessageTag::Have,
            5 => MessageTag::Bitfield,
            6 => MessageTag::Request,
            7 => MessageTag::Piece,
            8 => MessageTag::Cancel,
            tag => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Unknown message type {}.", tag),
                ))
            }
        };
        let data = if src.len() > 5 {
            src[5..4 + length].to_vec()
        } else {
            Vec::new()
        };
        src.advance(4 + length);
        Ok(Some(Message { tag, payload: data }))
    }
}

impl Encoder<Message> for MessageFramer {
    type Error = std::io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Don't send a string if it is longer than the other end will
        // accept.
        if item.payload.len() + 1 > MAX { // 1 extra for message_id which a single extra byte
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", item.payload.len() + 1)
            ));
        }

        // Convert the length into a byte array.
        // The cast to u32 cannot overflow due to the length check above.
        let len_slice = u32::to_be_bytes(item.payload.len() as u32 + 1);

        // Reserve space in the buffer.
        dst.reserve(4 + 1 + item.payload.len());

        // Write the length and string to the buffer.
        dst.extend_from_slice(&len_slice);
        dst.put_u8(item.tag as u8);
        dst.extend_from_slice(&item.payload);
        Ok(())
    }
}
