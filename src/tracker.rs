pub use peers::Peers;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct TrackerRequest {
    /// A string of length 20 which this downloader uses as its id.
    ///
    /// Each downloader generates its own id at random at the start of a new download. This value
    /// will also almost certainly have to be escaped.
    pub peer_id: String,

    /// The port number this peer is listening on.
    ///
    /// Common behavior is for a downloader to try to listen on port 6881 and if that port is taken
    /// try 6882, then 6883, etc. and give up after 6889.
    pub port: u16,

    /// The total amount uploaded so far, encoded in base ten ascii.
    pub uploaded: usize,

    /// The total amount downloaded so far, encoded in base ten ascii.
    pub downloaded: usize,

    /// The number of bytes this peer still has to download, encoded in base ten ascii.
    ///
    /// NOTE: that this can't be computed from downloaded and the file length since it might be a
    /// resume, and there's a chance that some of the downloaded data failed an integrity check and
    /// had to be re-downloaded.
    pub left: usize,

    /// Whether the peer list should use the compact representation.
    ///
    /// The compact representation is more commonly used in the wild, the non-compact
    /// representation is mostly supported for backward-compatibility.
    pub compact: u8,
}

impl TrackerRequest {
    pub fn new(left: usize) -> Self {
        Self {
            left,
            peer_id: "00112233445566778899".to_string(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            compact: 1,
        }
    }

    pub fn left(self, left: usize) -> Self {
        Self { left, ..self }
    }

    pub fn downloaded(self, downloaded: usize) -> Self {
        Self { downloaded, ..self }
    }

    pub fn uploaded(self, uploaded: usize) -> Self {
        Self { uploaded, ..self }
    }
}

/// Tracker responses are bencoded dictionaries.
/// TODO: implement the case of response failure
#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    /// The number of seconds the downloader should wait between regular rerequests.
    pub interval: usize,

    /// List of peers that your client can connect to.
    pub peers: Peers,
}

mod peers {
    use serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    };
    use std::{
        fmt,
        net::{Ipv4Addr, SocketAddrV4},
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Peers(pub Vec<SocketAddrV4>);
    struct PeersVisitor;

    impl<'de> Visitor<'de> for PeersVisitor {
        type Value = Peers;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(
                formatter,
                "6 bytes first 4 are a peer's IP address and the last 2 are their port"
            )
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 6 != 0 {
                return Err(E::custom(format!(
                    "length is {length}, {length} mod 20 = {remainder}",
                    length = v.len(),
                    remainder = v.len() % 20
                )));
            }

            // TODO: use [`std::slice::array_chunks`] when stable
            Ok(Peers(
                v.chunks_exact(6)
                    .map(|slice| {
                        SocketAddrV4::new(
                            Ipv4Addr::from(u32::from_be_bytes(slice[0..4].try_into().unwrap())),
                            u16::from_be_bytes(slice[4..].try_into().unwrap()),
                        )
                    })
                    .collect(),
            ))
        }
    }

    impl<'de> Deserialize<'de> for Peers {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PeersVisitor)
        }
    }

    impl Serialize for Peers {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut bytes = Vec::with_capacity(6 * self.0.len());

            for peer in self.0.iter() {
                bytes.extend(peer.ip().octets());
                bytes.extend(peer.port().to_be_bytes());
            }

            serializer.serialize_bytes(&bytes)
        }
    }
}
