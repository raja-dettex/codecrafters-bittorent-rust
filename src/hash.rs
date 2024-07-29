use serde::de::{Visitor, Deserialize, Deserializer};
    use serde::{de, Serialize};
    use std::fmt;
    pub struct HashStrVisitor;
    #[derive(Clone, Debug)]
    pub struct Hashes(pub Vec<[u8; 20]>);
    

    impl Serialize for Hashes {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer {
            let single_slice = self.0.concat();
            serializer.serialize_bytes(&single_slice)
        }
    }

    impl<'de> Visitor<'de> for HashStrVisitor {
        type Value = Hashes;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a byte string whose whose length is multiple of 20")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error, {
            if v.len() % 20 != 0 { 
                return Err(E::custom(format!("length is {}", v.len())));
            }
            let slices  = v.chunks_exact(20).map(|slice_20| slice_20.try_into().expect("")).collect();
            Ok(Hashes(slices))
            
        }
    }

    impl<'de> Deserialize<'de> for Hashes {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de> {
            deserializer.deserialize_bytes(HashStrVisitor)
        }
    }