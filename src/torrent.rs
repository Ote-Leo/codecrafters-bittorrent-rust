use serde::{Deserialize, Serialize};

pub use pieces::Pieces;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Torrent {
    // TODO: using a proper url
    /// The URL of the tracker.
    pub announce: String,

    pub info: Info,
}

impl Torrent {
    /// Calculate the total number of bytes for this torrent
    pub fn content_length(&self) -> usize {
        match self.info.content {
            Content::SingleFile { length } => length,
            Content::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Info {
    /// The suggested name to save the file (or directory) as. It is purely advisory.
    ///
    /// In the single file case, the name key is the name of a file, in the muliple file case, it's
    /// the name of a directory.
    pub name: String,

    /// The number of bytes in each piece the file is split into. For the purposes of transfer,
    /// files are split into fixed-size pieces which are all the same length except for possibly
    /// the last one which may be truncated. `piece_length` is almost always a power of two, most
    /// commonly $2^{18} = 256K$ (BitTorrent prior to version 3.2 uses $2^{20} = 1M$ as default).
    #[serde(rename = "piece length")]
    pub piece_length: usize,

    /// A bytestring whose length is a multiple of 20. It is to be subdivided into strings of
    /// length 20, each of which is the SHA1 hash of the piece at the corresponding index.
    pub pieces: Pieces,

    /// There is also a key length or a key files, but not both or neither. otherwise it represents
    /// a set of files which go in a directory structure.
    #[serde(flatten)]
    pub content: Content,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Content {
    /// The `length` of the file in bytes
    SingleFile { length: usize },

    /// For the purposes of the other keys in the [`Info`], the multi-file case is treated as only having a single
    /// file by concatenating the files in the order they appear in the files list.
    MultiFile { files: Vec<TorrentFile> },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TorrentFile {
    ///  The length of the file, in bytes.
    pub length: usize,

    /// A list of UTF-8 encoded strings corresponding to subdirectory names, the last of which is
    /// the actual file name (a zero length list is an error case).
    pub path: Vec<String>,
}

mod pieces {
    use serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize,
    };
    use std::fmt;

    #[derive(Debug, PartialEq, Eq)]
    pub struct Pieces(pub Vec<[u8; 20]>);
    struct PiecesVisitor;

    impl<'de> Visitor<'de> for PiecesVisitor {
        type Value = Pieces;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a byte string whose length is a multiple of 20")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 20 != 0 {
                return Err(E::custom(format!(
                    "length is {length}, {length} mod 20 = {remainder}",
                    length = v.len(),
                    remainder = v.len() % 20
                )));
            }

            // TODO: use [`std::slice::array_chunks`] when stable
            Ok(Pieces(
                v.chunks_exact(20)
                    .map(|slice| slice.try_into().expect("guaranteed to be divisible by 20"))
                    .collect(),
            ))
        }
    }

    impl<'de> Deserialize<'de> for Pieces {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PiecesVisitor)
        }
    }

    impl Serialize for Pieces {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let hashes = self.0.concat();
            serializer.serialize_bytes(&hashes)
        }
    }
}
