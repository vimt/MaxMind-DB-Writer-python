use crate::encoder::Encoder;
use crate::value::MmdbValue;
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::io::Write;
use std::iter::repeat;

pub struct MMDBWriter {
    ip_version: u8,
    database_type: String,
    languages: Vec<String>,
    description: HashMap<String, String>,
    root: Tree,
    ipv4_compatible: bool,
    encoder: Encoder,
}

#[derive(Default)]
enum Tree {
    #[default]
    Node([Option<Box<Tree>>; 2]),
    Leaf(MmdbValue),
}
impl Tree {
    fn children_mut(&mut self) -> &mut [Option<Box<Tree>>; 2] {
        match self {
            Tree::Node(children) => children,
            _ => panic!("Cannot get children of a leaf node"),
        }
    }
    fn is_leaf(&self) -> bool {
        matches!(self, Tree::Leaf(_))
    }
    fn leaf(self) -> MmdbValue {
        match self {
            Tree::Leaf(value) => value,
            _ => panic!("Cannot get value of a node"),
        }
    }
}

impl MMDBWriter {
    pub fn new(
        ip_version: u8,
        database_type: String,
        languages: Vec<String>,
        description: HashMap<String, String>,
        ipv4_compatible: bool,
    ) -> Self {
        MMDBWriter {
            ip_version,
            database_type,
            languages,
            description,
            root: Tree::default(),
            ipv4_compatible,
            encoder: Encoder::new(),
        }
    }

    pub fn insert_network(&mut self, network: IpNetwork, content: MmdbValue) -> Result<(), String> {
        let bits = match network {
            IpNetwork::V4(ipv4) => {
                if self.ip_version == 6 {
                    if !self.ipv4_compatible {
                        return Err(format!(
                            "You inserted a IPv4 address {network} to an IPv6 database."
                            "Please use ipv4_compatible=True option store "
                            "IPv4 address in IPv6 database as ::/96 format"
                        ));
                    }
                }
                ipv4.ip().to_bits() as u128
            }
            IpNetwork::V6(ipv6) => {
                if self.ip_version == 4 {
                    // TODO 专门的 error
                    return Err(format!(
                        "You inserted a IPv6 address {network} to an IPv4-only database."
                    ));
                }
                ipv6.ip().to_bits()
            }
        };

        let bit_len = if self.ip_version == 6 { 128 } else { 32 };
        let prefix_len = network.prefix() as usize;

        let mut node = self.root.children_mut();
        let mut supernet_leaf = None;

        for offset in (prefix_len..bit_len).rev() {
            let bit = ((bits >> offset) & 1) as usize;
            let is_last = offset == prefix_len;

            // If we have a supernet leaf, we need to insert it here
            if let Some(supernet_leaf) = supernet_leaf.take() {
                node[1 - bit] = Some(Box::new(Tree::Leaf(supernet_leaf)));
                break;
            }

            let next = node[bit].get_or_insert_with(Box::<Tree>::default);
            if next.is_leaf() {
                let value = std::mem::take(next);
                supernet_leaf = Some(value.leaf());
            }

            if is_last {
                node[bit] = Some(Box::new(Tree::Leaf(content)));
                break;
            }
            node = next.children_mut();
        }
        Ok(())
    }

    pub fn build(&self, out: &mut impl Write) -> Result<(), Box<dyn Error>> {
        // TODO: Implement database file writing logic
        Ok(())
    }

    fn write_search_tree(&mut self, buf: &mut impl Write) -> io::Result<(u32, u64)> {
        let mut node_count = 0;
        let mut stack = vec![&self.root];
        // let data_section_start = file.stream_position()?;

        Ok((node_count, 0))
    }

    fn write_data_section(&mut self, buf: &mut impl Write) -> io::Result<()> {
        // Write the encoded data
        buf.write_all(&self.encoder.data)?;
        Ok(())
    }

    fn write_metadata(
        &mut self,
        buf: &mut impl Write,
        node_count: u32,
        data_section_start: u64,
    ) -> io::Result<()> {
        const METADATA_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";
        buf.write_all(METADATA_MARKER)?;

        let metadata = MmdbValue::Map(HashMap::from([
            ("node_count".to_string(), MmdbValue::Uint32(node_count)),
            ("record_size".to_string(), MmdbValue::Uint16(32)), // Assuming 32-bit record size
            (
                "ip_version".to_string(),
                MmdbValue::Uint16(self.ip_version as u16),
            ),
            (
                "database_type".to_string(),
                MmdbValue::String(self.database_type.clone()),
            ),
            (
                "languages".to_string(),
                MmdbValue::Array(
                    self.languages
                        .iter()
                        .map(|lang| MmdbValue::String(lang.clone()))
                        .collect(),
                ),
            ),
            (
                "binary_format_major_version".to_string(),
                MmdbValue::Uint16(2),
            ),
            (
                "binary_format_minor_version".to_string(),
                MmdbValue::Uint16(0),
            ),
            (
                "build_epoch".to_string(),
                MmdbValue::Uint64(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                ),
            ),
            (
                "description".to_string(),
                MmdbValue::Map(
                    self.description
                        .iter()
                        .map(|(k, v)| (k.clone(), MmdbValue::String(v.clone())))
                        .collect(),
                ),
            ),
        ]));

        let encoded_metadata = self.encoder.encode(&metadata);
        buf.write_all(&encoded_metadata)?;

        Ok(())
    }
}
