# coding: utf-8
__version__ = '0.1.1'

import logging
import math
import struct
import time
from typing import Union

from netaddr import IPSet, IPNetwork

MMDBType = Union[dict, list, str, bytes, int, bool]

logger = logging.getLogger(__name__)

METADATA_MAGIC = b'\xab\xcd\xefMaxMind.com'


class SearchTreeNode(object):
    def __init__(self, left=None, right=None):
        self.left = left
        self.right = right

    def get_or_create(self, item):
        if item == 0:
            self.left = self.left or SearchTreeNode()
            return self.left
        elif item == 1:
            self.right = self.right or SearchTreeNode()
            return self.right

    def __getitem__(self, item):
        if item == 0:
            return self.left
        elif item == 1:
            return self.right

    def __setitem__(self, key, value):
        if key == 0:
            self.left = value
        elif key == 1:
            self.right = value


class SearchTreeLeaf(object):
    def __init__(self, value):
        self.value = value

    def __repr__(self):
        return "SearchTreeLeaf(value={value})".format(value=self.value)

    __str__ = __repr__


class Encoder(object):

    def __init__(self, cache=True):
        self.data_cache = {}
        self.data_list = []
        self.data_pointer = 0

        self.cache = cache

    def _encode_pointer(self, value):
        pointer = value
        if pointer >= 134744064:
            res = struct.pack('>BI', 0x38, pointer)
        elif pointer >= 526336:
            pointer -= 526336
            res = struct.pack('>BBBB', 0x30 + ((pointer >> 24) & 0x07),
                              (pointer >> 16) & 0xff, (pointer >> 8) & 0xff,
                              pointer & 0xff)
        elif pointer >= 2048:
            pointer -= 2048
            res = struct.pack('>BBB', 0x28 + ((pointer >> 16) & 0x07),
                              (pointer >> 8) & 0xff, pointer & 0xff)
        else:
            res = struct.pack('>BB', 0x20 + ((pointer >> 8) & 0x07),
                              pointer & 0xff)

        return res

    def _encode_utf8_string(self, value):
        encoded_value = value.encode('utf-8')
        res = self._make_header(2, len(encoded_value))
        res += encoded_value
        return res

    def _encode_bytes(self, value):
        return self._make_header(4, len(value)) + value

    def _encode_uint(self, type_id, max_len):
        def _encode_unsigned_value(value):
            res = b''
            while value != 0 and len(res) < max_len:
                res = struct.pack('>B', value & 0xff) + res
                value = value >> 8
            return self._make_header(type_id, len(res)) + res

        return _encode_unsigned_value

    def _encode_map(self, value):
        res = self._make_header(7, len(value))
        for k, v in list(value.items()):
            # Keys are always stored by value.
            res += self.encode(k)
            res += self.encode(v)
        return res

    def _encode_array(self, value):
        res = self._make_header(11, len(value))
        for k in value:
            res += self.encode(k)
        return res

    def _encode_boolean(self, value):
        return self._make_header(14, 1 if value else 0)

    def _encode_pack_type(self, type_id, fmt):
        def pack_type(value):
            res = struct.pack(fmt, value)
            return self._make_header(type_id, len(res)) + res

        return pack_type

    _type_decoder = None

    @property
    def type_decoder(self):
        if self._type_decoder is None:
            self._type_decoder = {
                1: self._encode_pointer,
                2: self._encode_utf8_string,
                3: self._encode_pack_type(3, '>d'),  # double,
                4: self._encode_bytes,
                5: self._encode_uint(5, 2),  # uint16
                6: self._encode_uint(6, 4),  # uint32
                7: self._encode_map,
                8: self._encode_pack_type(8, '>i'),  # int32
                9: self._encode_uint(9, 8),  # uint64
                10: self._encode_uint(10, 16),  # uint128
                11: self._encode_array,
                14: self._encode_boolean,
                15: self._encode_pack_type(15, '>f'),  # float,
            }
        return self._type_decoder

    def _make_header(self, type_id, length):
        if length >= 16843036:
            raise Exception('length >= 16843036')

        elif length >= 65821:
            five_bits = 31
            length -= 65821
            b3 = length & 0xff
            b2 = (length >> 8) & 0xff
            b1 = (length >> 16) & 0xff
            additional_length_bytes = struct.pack('>BBB', b1, b2, b3)

        elif length >= 285:
            five_bits = 30
            length -= 285
            b2 = length & 0xff
            b1 = (length >> 8) & 0xff
            additional_length_bytes = struct.pack('>BB', b1, b2)

        elif length >= 29:
            five_bits = 29
            length -= 29
            additional_length_bytes = struct.pack('>B', length & 0xff)

        else:
            five_bits = length
            additional_length_bytes = b''

        if type_id <= 7:
            res = struct.pack('>B', (type_id << 5) + five_bits)
        else:
            res = struct.pack('>BB', five_bits, type_id - 7)

        return res + additional_length_bytes

    _python_type_id = {
        float: 15,
        bool: 14,
        list: 11,
        dict: 7,
        bytes: 4,
        str: 2
    }

    def python_type_id(self, value):
        value_type = type(value)
        type_id = self._python_type_id.get(value_type)
        if type_id:
            return type_id
        if value_type is int:
            if value > 0xffffffffffffffff:
                return 10
            elif value > 0xffffffff:
                return 9
            elif value > 0xffff:
                return 6
            elif value < 0:
                return 8
            else:
                return 5
        raise TypeError("unknown type {value_type}".format(value_type=value_type))

    def encode_meta(self, meta):
        res = self._make_header(7, len(meta))
        meta_type = {'node_count': 6, 'record_size': 5, 'ip_version': 5,
                     'binary_format_major_version': 5, 'binary_format_minor_version': 5,
                     'build_epoch': 9}
        for k, v in list(meta.items()):
            # Keys are always stored by value.
            res += self.encode(k)
            res += self.encode(v, meta_type.get(k))
        return res

    def encode(self, value, type_id=None):
        if self.cache:
            try:
                return self.data_cache[id(value)]
            except KeyError:
                pass

        if not type_id:
            type_id = self.python_type_id(value)

        try:
            encoder = self.type_decoder[type_id]
        except KeyError:
            raise ValueError("unknown type_id={type_id}".format(type_id=type_id))
        res = encoder(value)

        if self.cache:
            # add to cache
            if type_id == 1:
                self.data_list.append(res)
                self.data_pointer += len(res)
                return res
            else:
                self.data_list.append(res)
                pointer_position = self.data_pointer
                self.data_pointer += len(res)
                pointer = self.encode(pointer_position, 1)
                self.data_cache[id(value)] = pointer
                return pointer
        return res


class TreeWriter(object):
    encoder_cls = Encoder

    def __init__(self, tree, meta):
        self._node_idx = {}
        self._leaf_offset = {}
        self._node_list = []
        self._node_counter = 0
        self._record_size = 0

        self.tree = tree
        self.meta = meta

        self.encoder = self.encoder_cls(cache=True)

    @property
    def _data_list(self):
        return self.encoder.data_list

    @property
    def _data_pointer(self):
        return self.encoder.data_pointer + 16

    def _build_meta(self):
        return {
            "node_count": self._node_counter,
            "record_size": self.record_size,
            **self.meta
        }

    def _adjust_record_size(self):
        # Tree records should be large enough to contain either tree node index
        # or data offset.
        max_id = self._node_counter + self._data_pointer + 1

        # Estimate required bit count.
        bit_count = int(math.ceil(math.log(max_id, 2)))
        if bit_count <= 24:
            self.record_size = 24
        elif bit_count <= 28:
            self.record_size = 28
        elif bit_count <= 32:
            self.record_size = 32
        else:
            raise Exception('record_size > 32')

        self.data_offset = self.record_size * 2 / 8 * self._node_counter

    def _enumerate_nodes(self, node):
        if type(node) is SearchTreeNode:
            node_id = id(node)
            if node_id not in self._node_idx:
                self._node_idx[node_id] = self._node_counter
                self._node_counter += 1
                self._node_list.append(node)

            self._enumerate_nodes(node.left)
            self._enumerate_nodes(node.right)

        elif type(node) is SearchTreeLeaf:
            node_id = id(node)
            if node_id not in self._leaf_offset:
                res = self.encoder.encode(node.value)
                self._leaf_offset[node_id] = self._data_pointer - len(res)
        else:  # == None
            return

    def _calc_record_idx(self, node):
        if node is None:
            return self._node_counter
        elif type(node) is SearchTreeNode:
            return self._node_idx[id(node)]
        elif type(node) is SearchTreeLeaf:
            return self._leaf_offset[id(node)] + self._node_counter
        else:
            raise Exception("unexpected type")

    def _cal_node_bytes(self, node) -> bytes:
        left_idx = self._calc_record_idx(node.left)
        right_idx = self._calc_record_idx(node.right)

        if self.record_size == 24:
            b1 = (left_idx >> 16) & 0xff
            b2 = (left_idx >> 8) & 0xff
            b3 = left_idx & 0xff
            b4 = (right_idx >> 16) & 0xff
            b5 = (right_idx >> 8) & 0xff
            b6 = right_idx & 0xff
            return struct.pack('>BBBBBB', b1, b2, b3, b4, b5, b6)

        elif self.record_size == 28:
            b1 = (left_idx >> 16) & 0xff
            b2 = (left_idx >> 8) & 0xff
            b3 = left_idx & 0xff
            b4 = ((left_idx >> 24) & 0xf) * 16 + \
                 ((right_idx >> 24) & 0xf)
            b5 = (right_idx >> 16) & 0xff
            b6 = (right_idx >> 8) & 0xff
            b7 = right_idx & 0xff
            return struct.pack('>BBBBBBB', b1, b2, b3, b4, b5, b6, b7)

        elif self.record_size == 32:
            return struct.pack('>II', left_idx, right_idx)

        else:
            raise Exception('self.record_size > 32')

    def write(self, fname):
        self._enumerate_nodes(self.tree)
        self._adjust_record_size()

        with open(fname, 'wb') as f:
            for node in self._node_list:
                f.write(self._cal_node_bytes(node))

            f.write(b'\x00' * 16)

            for element in self._data_list:
                f.write(element)

            f.write(METADATA_MAGIC)
            f.write(self.encoder_cls(cache=False).encode_meta(self._build_meta()))


def bits_rstrip(n, length=None, keep=0):
    return map(int, bin(n)[2:].rjust(length, '0')[:keep])


class MMDBWriter(object):

    def __init__(self, ip_version=4, database_type='GeoIP',
                 languages=None, description='GeoIP db',
                 ipv4_compatible=False):
        self.tree = SearchTreeNode()
        self.ipv4_compatible = ipv4_compatible

        if languages is None:
            languages = []
        self.description = description
        self.database_type = database_type
        self.ip_version = ip_version
        self.languages = languages
        self.binary_format_major_version = 2
        self.binary_format_minor_version = 0

        self._bit_length = 128 if ip_version == 6 else 32

        if ip_version not in [4, 6]:
            raise ValueError("ip_version should be 4 or 6, {} is incorrect".format(ip_version))
        if ip_version == 4 and ipv4_compatible:
            raise ValueError("ipv4_compatible=True can set when ip_version=6")
        if not self.binary_format_major_version:
            raise ValueError("major_version can't be empty or 0: {}".format(self.binary_format_major_version))
        if isinstance(description, str):
            self.description = {i: description for i in languages}
        for i in languages:
            if i not in self.description:
                raise ValueError("language {} must have description!")

    def insert_network(self, network: IPSet, content: MMDBType):
        leaf = SearchTreeLeaf(content)
        if not isinstance(network, IPSet):
            raise ValueError("network type should be netaddr.IPSet.")
        network = network.iter_cidrs()
        for cidr in network:
            if self.ip_version == 4 and cidr.version == 6:
                raise ValueError('You inserted a IPv6 address {} '
                                 'to an IPv4-only database.'.format(cidr))
            if self.ip_version == 6 and cidr.version == 4:
                if not self.ipv4_compatible:
                    raise ValueError("You inserted a IPv4 address {} to an IPv6 database."
                                     "Please use ipv4_compatible=True option store "
                                     "IPv4 address in IPv6 database as ::/96 format".format(cidr))
                cidr = cidr.ipv6(True)
            node = self.tree
            bits = list(bits_rstrip(cidr.value, self._bit_length, cidr.prefixlen))
            current_node = node
            supernet_leaf = None  # Tracks whether we are inserting into a subnet
            for (index, ip_bit) in enumerate(bits[:-1]):
                previous_node = current_node
                current_node = previous_node.get_or_create(ip_bit)

                if isinstance(current_node, SearchTreeLeaf):
                    current_cidr = IPNetwork((int(''.join(map(str, bits[:index + 1])).ljust(self._bit_length, '0'), 2), index + 1))
                    logger.info(f"Inserting {cidr} ({content}) into subnet of {current_cidr} ({current_node.value})")
                    supernet_leaf = current_node
                    current_node = SearchTreeNode()
                    previous_node[ip_bit] = current_node

                if supernet_leaf:
                    next_bit = bits[index + 1]
                    # Insert supernet information on each inverse bit of the current subnet
                    current_node[1 - next_bit] = supernet_leaf
            current_node[bits[-1]] = leaf

    def to_db_file(self, filename: str):
        return TreeWriter(self.tree, self._build_meta()).write(filename)

    def _build_meta(self):
        return {
            "ip_version": self.ip_version,
            "database_type": self.database_type,
            "languages": self.languages,
            "binary_format_major_version": self.binary_format_major_version,
            "binary_format_minor_version": self.binary_format_minor_version,
            "build_epoch": int(time.time()),
            "description": self.description,
        }
