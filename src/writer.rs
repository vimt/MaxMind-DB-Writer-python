use crate::encoder::Encoder;
use crate::value::MmdbValue;
use byteorder::{BigEndian, WriteBytesExt};
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::io::Write;

const METADATA_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";
const DATA_SECTION_SEPARATOR: u32 = 16;
const UNASSIGNED_ID: u32 = u32::MAX;

pub struct MMDBWriter {
    ip_version: u8,
    database_type: String,
    languages: Vec<String>,
    description: HashMap<String, String>,
    root: Tree,
    ipv4_compatible: bool,
    encoder: Encoder,
}

pub(crate) enum Tree {
    Node {
        children: [Option<Box<Tree>>; 2],
        id: u32,
    },
    Leaf(MmdbValue),
}

impl Default for Tree {
    fn default() -> Self {
        Tree::Node {
            children: [None, None],
            id: UNASSIGNED_ID,
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

    fn parse_network(&self, network: IpNetwork) -> Result<(u128, u8, u8), String> {
        match network {
            IpNetwork::V4(v4) => {
                if self.ip_version == 6 && self.ipv4_compatible {
                    let octets = v4.ip().to_ipv6_mapped().octets();
                    Ok((u128::from_be_bytes(octets), v4.prefix(), 128))
                } else if self.ip_version == 4 {
                    let octets = v4.ip().octets();
                    let mut bytes = [0u8; 16];
                    bytes[12..16].copy_from_slice(&octets);
                    Ok((u128::from_be_bytes(bytes), v4.prefix(), 32))
                } else {
                    Err("Invalid IP version for IPv4 network".to_string())
                }
            }
            IpNetwork::V6(v6) => {
                if self.ip_version == 4 {
                    Err("Cannot insert IPv6 into IPv4-only database".to_string())
                } else {
                    Ok((u128::from(v6.ip()), v6.prefix(), 128))
                }
            }
        }
    }

    pub fn insert_network(
        &mut self,
        network: IpNetwork,
        content: MmdbValue,
    ) -> Result<(), String> {
        let (ip, prefix_len, total_bits) = self.parse_network(network)?;

        if prefix_len == 0 {
            self.root = Tree::Leaf(content);
            return Ok(());
        }

        let get_bit = |idx: u8| -> usize {
            ((ip >> (total_bits - 1 - idx)) & 1) as usize
        };

        // Raw pointer used to work around borrow checker limitations with
        // mutable tree traversal + sibling modification at each level.
        let mut node_ptr = &mut self.root as *mut Tree;
        let mut supernet: Option<MmdbValue> = None;

        for idx in 0..prefix_len {
            let bit = get_bit(idx);
            let is_last = idx == prefix_len - 1;
            let node = unsafe { &mut *node_ptr };

            // If current node is a leaf, convert to internal node
            if let Tree::Leaf(val) = node {
                supernet = Some(val.clone());
                *node = Tree::default();
            }

            let children = match node {
                Tree::Node { children, .. } => children,
                _ => unreachable!(),
            };

            if is_last {
                children[bit] = Some(Box::new(Tree::Leaf(content)));
                return Ok(());
            }

            // Extract leaf value from child before mutating
            let child_leaf_val = children[bit].as_ref().and_then(|c| {
                if let Tree::Leaf(val) = c.as_ref() {
                    Some(val.clone())
                } else {
                    None
                }
            });

            if let Some(val) = child_leaf_val {
                supernet = Some(val);
                children[bit] = Some(Box::new(Tree::default()));
            } else if children[bit].is_none() {
                children[bit] = Some(Box::new(Tree::default()));
            }

            // Advance to child node
            node_ptr = children[bit].as_mut().unwrap().as_mut() as *mut Tree;

            // Propagate supernet: in the child node, set the sibling of the
            // direction we'll take next. This ensures that all branches NOT
            // on the path to the subnet retain the supernet value.
            if let Some(ref supernet_val) = supernet {
                let next_bit = get_bit(idx + 1);
                let child = unsafe { &mut *node_ptr };
                match child {
                    Tree::Node { children, .. } => {
                        children[1 - next_bit] =
                            Some(Box::new(Tree::Leaf(supernet_val.clone())));
                    }
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }

    pub fn build(&mut self, out: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let mut root = std::mem::replace(&mut self.root, Tree::default());

        let mut ordered_nodes: Vec<*const Tree> = Vec::new();
        let mut next_id: u32 = 0;
        let mut leaf_offsets: HashMap<*const Tree, u32> = HashMap::new();
        Self::assign_ids_and_encode(
            &mut root,
            &mut self.encoder,
            &mut next_id,
            &mut ordered_nodes,
            &mut leaf_offsets,
        );
        let node_count = next_id;

        let data_size = self.encoder.get_data_segment().len() as u32;
        let max_record = node_count + DATA_SECTION_SEPARATOR + data_size;
        let record_size_bits: u8 = if max_record <= 0xFF_FFFF {
            24
        } else if max_record <= 0x0FFF_FFFF {
            28
        } else {
            32
        };

        let record_bytes_per_node = (record_size_bits as usize) * 2 / 8;
        let mut tree_buf = Vec::with_capacity(ordered_nodes.len() * record_bytes_per_node);
        for &node_ptr in &ordered_nodes {
            let node = unsafe { &*node_ptr };
            if let Tree::Node { children, .. } = node {
                let left = Self::record_value(&children[0], node_count, &leaf_offsets);
                let right = Self::record_value(&children[1], node_count, &leaf_offsets);
                Self::write_record_pair(&mut tree_buf, left, right, record_size_bits)?;
            }
        }
        out.write_all(&tree_buf)?;
        out.write_all(&[0u8; DATA_SECTION_SEPARATOR as usize])?;
        out.write_all(self.encoder.get_data_segment())?;
        self.write_metadata(out, node_count, record_size_bits)?;

        self.root = root;
        Ok(())
    }

    fn assign_ids_and_encode(
        node: &mut Tree,
        encoder: &mut Encoder,
        next_id: &mut u32,
        ordered: &mut Vec<*const Tree>,
        leaf_offsets: &mut HashMap<*const Tree, u32>,
    ) {
        let mut stack: Vec<*mut Tree> = vec![node as *mut Tree];

        while let Some(ptr) = stack.pop() {
            let n = unsafe { &mut *ptr };
            match n {
                Tree::Node { children, id } => {
                    if *id == UNASSIGNED_ID {
                        *id = *next_id;
                        *next_id += 1;
                        ordered.push(ptr as *const Tree);

                        let right_ptr =
                            children[1].as_mut().map(|c| c.as_mut() as *mut Tree);
                        let left_ptr =
                            children[0].as_mut().map(|c| c.as_mut() as *mut Tree);

                        if let Some(p) = right_ptr {
                            stack.push(p);
                        }
                        if let Some(p) = left_ptr {
                            stack.push(p);
                        }
                    }
                }
                Tree::Leaf(value) => {
                    let leaf_ptr = ptr as *const Tree;
                    if !leaf_offsets.contains_key(&leaf_ptr) {
                        let offset = encoder.encode_leaf(value);
                        leaf_offsets.insert(leaf_ptr, offset);
                    }
                }
            }
        }
    }

    fn record_value(
        child: &Option<Box<Tree>>,
        node_count: u32,
        leaf_offsets: &HashMap<*const Tree, u32>,
    ) -> u32 {
        match child {
            None => node_count,
            Some(child_box) => match child_box.as_ref() {
                Tree::Node { id, .. } => *id,
                Tree::Leaf(_) => {
                    let ptr = child_box.as_ref() as *const Tree;
                    let offset = leaf_offsets
                        .get(&ptr)
                        .expect("Leaf offset not found");
                    node_count + DATA_SECTION_SEPARATOR + offset
                }
            },
        }
    }

    fn write_record_pair(
        out: &mut impl Write,
        left: u32,
        right: u32,
        record_size: u8,
    ) -> io::Result<()> {
        match record_size {
            24 => {
                out.write_u8(((left >> 16) & 0xFF) as u8)?;
                out.write_u8(((left >> 8) & 0xFF) as u8)?;
                out.write_u8((left & 0xFF) as u8)?;
                out.write_u8(((right >> 16) & 0xFF) as u8)?;
                out.write_u8(((right >> 8) & 0xFF) as u8)?;
                out.write_u8((right & 0xFF) as u8)?;
            }
            28 => {
                out.write_u8(((left >> 16) & 0xFF) as u8)?;
                out.write_u8(((left >> 8) & 0xFF) as u8)?;
                out.write_u8((left & 0xFF) as u8)?;
                out.write_u8(
                    (((left >> 24) & 0x0F) << 4 | ((right >> 24) & 0x0F)) as u8,
                )?;
                out.write_u8(((right >> 16) & 0xFF) as u8)?;
                out.write_u8(((right >> 8) & 0xFF) as u8)?;
                out.write_u8((right & 0xFF) as u8)?;
            }
            32 => {
                out.write_u32::<BigEndian>(left)?;
                out.write_u32::<BigEndian>(right)?;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unsupported record size: {record_size} bits"),
                ));
            }
        }
        Ok(())
    }

    fn write_metadata(
        &self,
        out: &mut impl Write,
        node_count: u32,
        record_size: u8,
    ) -> io::Result<()> {
        out.write_all(METADATA_MARKER)?;

        // Ordered: build_epoch LAST to avoid libmaxminddb v1.13 issue
        // where an empty map/array at the tail triggers MMDB_INVALID_DATA_ERROR.
        let entries: Vec<(&str, MmdbValue)> = vec![
            ("node_count", MmdbValue::Uint32(node_count)),
            ("record_size", MmdbValue::Uint16(record_size as u16)),
            ("ip_version", MmdbValue::Uint16(self.ip_version as u16)),
            (
                "database_type",
                MmdbValue::String(self.database_type.clone()),
            ),
            (
                "languages",
                MmdbValue::Array(
                    self.languages
                        .iter()
                        .map(|l| MmdbValue::String(l.clone()))
                        .collect(),
                ),
            ),
            ("binary_format_major_version", MmdbValue::Uint16(2)),
            ("binary_format_minor_version", MmdbValue::Uint16(0)),
            (
                "description",
                MmdbValue::Map(
                    self.description
                        .iter()
                        .map(|(k, v)| (k.clone(), MmdbValue::String(v.clone())))
                        .collect(),
                ),
            ),
            (
                "build_epoch",
                MmdbValue::Uint64(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                ),
            ),
        ];

        let metadata_bytes = Encoder::encode_ordered_map_inline(&entries);
        out.write_all(&metadata_bytes)?;
        Ok(())
    }
}
