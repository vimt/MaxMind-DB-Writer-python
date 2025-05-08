use crate::encoder::Encoder;
use crate::value::MmdbValue;
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::io::Write;
use std::iter::repeat;
use std::cell::Cell;
use byteorder::{BigEndian, WriteBytesExt};

pub struct MMDBWriter {
    ip_version: u8,
    database_type: String,
    languages: Vec<String>,
    description: HashMap<String, String>,
    root: Tree,
    ipv4_compatible: bool,
    encoder: Encoder,
}

enum Tree {
    Node { children: [Option<Box<Tree>>; 2], id: Cell<Option<u32>> },
    Leaf(MmdbValue),
}

impl Default for Tree {
    fn default() -> Self {
        Tree::Node { children: [None, None], id: Cell::new(None) }
    }
}

impl Tree {
    fn children_mut(&mut self) -> &mut [Option<Box<Tree>>; 2] {
        match self {
            Tree::Node { children, .. } => children,
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
    fn get_id(&self) -> Option<u32> {
        match self {
            Tree::Node { id, .. } => id.get(),
            _ => None,
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
            root: Tree::Node { children: [None, None], id: Cell::new(None) },
            ipv4_compatible,
            encoder: Encoder::new(),
        }
    }

    pub fn insert_network(&mut self, network: IpNetwork, content: MmdbValue) -> Result<(), String> {
        let (ip_as_u128, actual_prefix_len, total_bit_len) = match network {
            IpNetwork::V4(ipv4_network) => {
                if self.ip_version == 6 && self.ipv4_compatible {
                    // Map IPv4 to IPv4-mapped IPv6
                    // Convert IPv4-mapped IPv6 octets to u128
                    let octets = ipv4_network.ip().to_ipv6_mapped().octets();
                    let ip_value = u128::from_be_bytes(octets);
                    Ok((ip_value, ipv4_network.prefix(), 128))
                } else if self.ip_version == 4 {
                    // Convert IPv4 octets to u128
                    let octets = ipv4_network.ip().octets();
                    let mut bytes = [0u8; 16];
                    bytes[12..16].copy_from_slice(&octets);
                    let ip_value = u128::from_be_bytes(bytes);
                    Ok((ip_value, ipv4_network.prefix(), 32))
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid IP version for IPv4 network").to_string())
                }
            }
            IpNetwork::V6(ipv6_network) => {
                if self.ip_version == 4 {
                    Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot insert IPv6 address into an IPv4-only database").to_string())
                } else {
                    Ok((ipv6_network.ip().into(), ipv6_network.prefix(), 128))
                }
            }
        }?;

        if actual_prefix_len == 0 {
            if let Tree::Node { children: _, id: _ } = &self.root {
                // If there was a more specific entry that became a supernet_leaf for /0.
                // This case is complex. Python's behavior for /0:
                // bits[:-1] is empty. current_node = self.tree. current_node[bits[-1]] ( problematic for /0 )
                // For /0, the Python code would effectively try current_node.get_or_create(???)
                // Let's assume /0 means the entire DB tree root IS this leaf.
                // Any existing structure under self.root would be implicitly "overwritten"
                // by this /0 entry, though specific sub-entries might persist if not handled.
                // For simplicity, a /0 replaces the root.
            }
            self.root = Tree::Leaf(content);
            return Ok(());
        }
        
        let mut current_node: &mut Tree = &mut self.root;
        let mut displaced_supernet_leaf_value: Option<MmdbValue> = None;

        for bit_idx in 0..actual_prefix_len {
            let current_bit = ((ip_as_u128 >> (total_bit_len - 1 - bit_idx)) & 1) as usize;
            let is_last_bit_of_prefix = bit_idx == actual_prefix_len - 1;

            if let Tree::Leaf(existing_leaf_val) = current_node {
                displaced_supernet_leaf_value = Some(existing_leaf_val.clone());
                *current_node = Tree::Node { children: [None, None], id: Cell::new(None) };
            }

            let children_array = match current_node {
                Tree::Node { children, .. } => children,
                _ => unreachable!("current_node should be a Node at this point"),
            };

            if let Some(supernet_val) = displaced_supernet_leaf_value.take() {
                children_array[1 - current_bit] = Some(Box::new(Tree::Leaf(supernet_val)));
            }

            let child_node_slot = &mut children_array[current_bit];

            if is_last_bit_of_prefix {
                *child_node_slot = Some(Box::new(Tree::Leaf(content.clone())));
                return Ok(());
            } else {
                match child_node_slot {
                    Some(ref mut existing_child_box) => {
                        if let Tree::Leaf(child_leaf_val) = existing_child_box.as_mut() {
                            displaced_supernet_leaf_value = Some(child_leaf_val.clone());
                            **existing_child_box = Tree::Node { children: [None, None], id: Cell::new(None) }; 
                        }
                    }
                    None => {
                        *child_node_slot = Some(Box::new(Tree::Node { children: [None, None], id: Cell::new(None) }));
                    }
                }
                current_node = child_node_slot.as_mut().unwrap().as_mut();
            }
        }
        Ok(())
    }

    pub fn build(&mut self, out: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let mut internal_node_count = 0;
        let mut ordered_internal_nodes: Vec<*const Tree> = Vec::new();
        
        let mut root = std::mem::replace(&mut self.root, Tree::Node { children: [None, None], id: Cell::new(None) });
        
        MMDBWriter::assign_node_ids_and_encode_data_static(
            &mut root, 
            &mut self.encoder,
            &mut internal_node_count, 
            &mut ordered_internal_nodes
        );
        
        self.root = root;

        let data_segment_size = self.encoder.get_data_segment().len() as u32;
        let max_record_value = internal_node_count + data_segment_size;

        let record_size_bits = if max_record_value <= 0xFFFFFF {
            24
        } else if max_record_value <= 0x0FFFFFFF {
            28
        } else {
            32
        };
        if max_record_value > 0xFFFFFFFF {
            return Err(Box::new(io::Error::new(io::ErrorKind::Other, "Database too large for 32-bit record pointers.")));
        }

        for node_ptr in ordered_internal_nodes {
            let node = unsafe { &*node_ptr }; 
            if let Tree::Node { children, .. } = node {
                let left_record = self.calculate_record_value(&children[0], internal_node_count);
                let right_record = self.calculate_record_value(&children[1], internal_node_count);
                self.write_tree_record(out, left_record, right_record, record_size_bits)?;
            } else {
                return Err(Box::new(io::Error::new(io::ErrorKind::Other, "Internal error: expected Tree::Node in ordered list.")));
            }
        }

        out.write_all(&[0u8; 16])?;

        out.write_all(self.encoder.get_data_segment())?;

        self.write_metadata(out, internal_node_count, record_size_bits)?;

        Ok(())
    }

    fn calculate_record_value(&self, child_node_opt: &Option<Box<Tree>>, internal_node_count: u32) -> u32 {
        match child_node_opt {
            Some(child_box) => {
                match child_box.as_ref() {
                    Tree::Node { id, .. } => {
                        id.get().expect("Internal node without ID during tree writing phase.")
                    }
                    Tree::Leaf(mmdb_val) => {
                        let offset = self.encoder.get_offset(mmdb_val)
                            .expect("Leaf value not found in encoder cache during tree writing.");
                        internal_node_count + offset
                    }
                }
            }
            None => internal_node_count,
        }
    }

    fn assign_node_ids_and_encode_data_static(
        current_node: &mut Tree,
        encoder: &mut Encoder,
        next_internal_node_id: &mut u32,
        ordered_internal_nodes: &mut Vec<*const Tree>
    ) {
        let mut stack = vec![current_node as *mut Tree];

        while let Some(node_ptr) = stack.pop() {
            let node = unsafe { &mut *node_ptr };

            match node {
                Tree::Node { children: _, id } => {
                    if id.get().is_none() {
                        id.set(Some(*next_internal_node_id));
                        ordered_internal_nodes.push(node_ptr as *const Tree);
                        *next_internal_node_id += 1;
                        
                        let left_child_ptr = if let Tree::Node { children, .. } = node {
                            children[0].as_mut().map(|child| child.as_mut() as *mut Tree)
                        } else {
                            None
                        };
                        
                        let right_child_ptr = if let Tree::Node { children, .. } = node {
                            children[1].as_mut().map(|child| child.as_mut() as *mut Tree)
                        } else {
                            None
                        };
                        
                        if let Some(right_ptr) = right_child_ptr {
                            stack.push(right_ptr);
                        }
                        
                        if let Some(left_ptr) = left_child_ptr {
                            stack.push(left_ptr);
                        }
                    }
                }
                Tree::Leaf(mmdb_value) => {
                    let _ = encoder.encode_value(mmdb_value);
                }
            }
        }
    }

    fn write_tree_record(
        &self, 
        out: &mut impl Write, 
        left_record: u32, 
        right_record: u32, 
        record_size_bits: u8
    ) -> io::Result<()> {
        match record_size_bits {
            24 => {
                out.write_u8(((left_record >> 16) & 0xFF) as u8)?;
                out.write_u8(((left_record >> 8) & 0xFF) as u8)?;
                out.write_u8((left_record & 0xFF) as u8)?;
                out.write_u8(((right_record >> 16) & 0xFF) as u8)?;
                out.write_u8(((right_record >> 8) & 0xFF) as u8)?;
                out.write_u8((right_record & 0xFF) as u8)?;
            }
            28 => {
                out.write_u8(((left_record >> 16) & 0xFF) as u8)?;
                out.write_u8(((left_record >> 8) & 0xFF) as u8)?;
                out.write_u8((left_record & 0xFF) as u8)?;
                let combined_nibble_byte = (((left_record >> 24) & 0x0F) << 4 | ((right_record >> 24) & 0x0F)) as u8;
                out.write_u8(combined_nibble_byte)?;
                out.write_u8(((right_record >> 16) & 0xFF) as u8)?;
                out.write_u8(((right_record >> 8) & 0xFF) as u8)?;
                out.write_u8((right_record & 0xFF) as u8)?;
            }
            32 => {
                out.write_u32::<BigEndian>(left_record)?;
                out.write_u32::<BigEndian>(right_record)?;
            }
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, 
                    format!("Unsupported record size: {} bits", record_size_bits)));
            }
        }
        Ok(())
    }

    fn write_search_tree(&mut self, out: &mut impl Write) -> io::Result<(u32, u64)> {
        let mut node_count = 0;
        let mut stack = vec![&self.root];
        // let data_section_start = file.stream_position()?;

        Ok((node_count, 0))
    }

    fn write_data_section(&mut self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_all(self.encoder.get_data_segment())?;
        Ok(())
    }

    fn write_metadata(
        &mut self,
        buf: &mut impl Write,
        node_count: u32,
        record_size: u8,
    ) -> io::Result<()> {
        const METADATA_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";
        buf.write_all(METADATA_MARKER)?;

        let metadata_map = HashMap::from([
            ("node_count".to_string(), MmdbValue::Uint32(node_count)),
            ("record_size".to_string(), MmdbValue::Uint16(record_size as u16)),
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
        ]);

        let mut meta_encoder = Encoder::new();
        let encoded_metadata = meta_encoder.encode_value(&MmdbValue::Map(metadata_map));
        buf.write_all(&encoded_metadata)?;

        Ok(())
    }
}
