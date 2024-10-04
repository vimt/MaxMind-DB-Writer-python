__version__ = "0.2.5"

import logging
import math
import struct
import time
from decimal import Decimal
from enum import IntEnum
from typing import Dict, List, Literal, Union

from netaddr import IPNetwork, IPSet


class MmdbBaseType:
    def __init__(self, value):
        self.value = value


# type hint
class MmdbF32(MmdbBaseType):
    def __init__(self, value: float):
        super().__init__(value)


class MmdbF64(MmdbBaseType):
    def __init__(self, value: Union[float, Decimal]):
        super().__init__(value)


class MmdbI32(MmdbBaseType):
    def __init__(self, value: int):
        super().__init__(value)


class MmdbU16(MmdbBaseType):
    def __init__(self, value: int):
        super().__init__(value)


class MmdbU32(MmdbBaseType):
    def __init__(self, value: int):
        super().__init__(value)


class MmdbU64(MmdbBaseType):
    def __init__(self, value: int):
        super().__init__(value)


class MmdbU128(MmdbBaseType):
    def __init__(self, value: int):
        super().__init__(value)


MMDBType = Union[
    dict,
    list,
    str,
    bytes,
    int,
    bool,
    MmdbF32,
    MmdbF64,
    MmdbI32,
    MmdbU16,
    MmdbU32,
    MmdbU64,
    MmdbU128,
]

logger = logging.getLogger(__name__)

METADATA_MAGIC = b"\xab\xcd\xefMaxMind.com"


class MMDBTypeID(IntEnum):
    POINTER = 1
    STRING = 2
    DOUBLE = 3
    BYTES = 4
    UINT16 = 5
    UINT32 = 6
    MAP = 7
    INT32 = 8
    UINT64 = 9
    UINT128 = 10
    ARRAY = 11
    DATA_CACHE = 12
    END_MARKER = 13
    BOOLEAN = 14
    FLOAT = 15


UINT16_MAX = 0xFFFF
UINT32_MAX = 0xFFFFFFFF
UINT64_MAX = 0xFFFFFFFFFFFFFFFF


class SearchTreeNode:
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


class SearchTreeLeaf:
    def __init__(self, value):
        self.value = value

    def __repr__(self):
        return f"SearchTreeLeaf(value={self.value})"

    __str__ = __repr__


IntType = Union[
    Literal[
        "auto",
        "u16",
        "u32",
        "u64",
        "u128",
        "i32",
        "uint16",
        "uint32",
        "uint64",
        "uint128",
        "int32",
    ],
    MmdbU16,
    MmdbU32,
    MmdbU64,
    MmdbU128,
    MmdbI32,
]
FloatType = Union[Literal["f32", "f64", "float32", "float64"], MmdbF32, MmdbF64]


class Encoder:
    def __init__(
        self, cache=True, int_type: IntType = "auto", float_type: FloatType = "f64"
    ):
        self.cache = cache
        self.int_type = int_type
        self.float_type = float_type

        self.data_cache = {}
        self.data_list = []
        self.data_pointer = 0
        self._python_type_id = {
            float: MMDBTypeID.DOUBLE,
            bool: MMDBTypeID.BOOLEAN,
            list: MMDBTypeID.ARRAY,
            dict: MMDBTypeID.MAP,
            bytes: MMDBTypeID.BYTES,
            str: MMDBTypeID.STRING,
            MmdbF32: MMDBTypeID.FLOAT,
            MmdbF64: MMDBTypeID.DOUBLE,
            MmdbI32: MMDBTypeID.INT32,
            MmdbU16: MMDBTypeID.UINT16,
            MmdbU32: MMDBTypeID.UINT32,
            MmdbU64: MMDBTypeID.UINT64,
            MmdbU128: MMDBTypeID.UINT128,
        }

    def _encode_pointer(self, value):
        pointer = value
        if pointer >= 134744064:
            res = struct.pack(">BI", 0x38, pointer)
        elif pointer >= 526336:
            pointer -= 526336
            res = struct.pack(
                ">BBBB",
                0x30 + ((pointer >> 24) & 0x07),
                (pointer >> 16) & 0xFF,
                (pointer >> 8) & 0xFF,
                pointer & 0xFF,
            )
        elif pointer >= 2048:
            pointer -= 2048
            res = struct.pack(
                ">BBB",
                0x28 + ((pointer >> 16) & 0x07),
                (pointer >> 8) & 0xFF,
                pointer & 0xFF,
            )
        else:
            res = struct.pack(">BB", 0x20 + ((pointer >> 8) & 0x07), pointer & 0xFF)

        return res

    def _encode_utf8_string(self, value):
        encoded_value = value.encode("utf-8")
        res = self._make_header(MMDBTypeID.STRING, len(encoded_value))
        res += encoded_value
        return res

    def _encode_bytes(self, value):
        return self._make_header(MMDBTypeID.BYTES, len(value)) + value

    def _encode_uint(self, type_id, max_len):
        value_max = 2 ** (max_len * 8)

        def _encode_unsigned_value(value):
            if value < 0 or value >= value_max:
                raise ValueError(
                    f"encode uint{max_len * 8} fail: "
                    f"{value} not in range(0, {value_max})"
                )
            res = b""
            while value != 0 and len(res) < max_len:
                res = struct.pack(">B", value & 0xFF) + res
                value = value >> 8
            return self._make_header(type_id, len(res)) + res

        return _encode_unsigned_value

    def _encode_map(self, value):
        res = self._make_header(MMDBTypeID.MAP, len(value))
        for k, v in list(value.items()):
            # Keys are always stored by value.
            res += self.encode(k)
            res += self.encode(v)
        return res

    def _encode_array(self, value):
        res = self._make_header(MMDBTypeID.ARRAY, len(value))
        for k in value:
            res += self.encode(k)
        return res

    def _encode_boolean(self, value):
        return self._make_header(MMDBTypeID.BOOLEAN, 1 if value else 0)

    def _encode_pack_type(self, type_id, fmt):
        def pack_type(value):
            res = struct.pack(fmt, value)
            return self._make_header(type_id, len(res)) + res

        return pack_type

    _type_encoder = None

    @property
    def type_encoder(self):
        if self._type_encoder is None:
            self._type_encoder = {
                MMDBTypeID.POINTER: self._encode_pointer,
                MMDBTypeID.STRING: self._encode_utf8_string,
                MMDBTypeID.DOUBLE: self._encode_pack_type(MMDBTypeID.DOUBLE, ">d"),
                MMDBTypeID.BYTES: self._encode_bytes,
                MMDBTypeID.UINT16: self._encode_uint(MMDBTypeID.UINT16, 2),
                MMDBTypeID.UINT32: self._encode_uint(MMDBTypeID.UINT32, 4),
                MMDBTypeID.MAP: self._encode_map,
                MMDBTypeID.INT32: self._encode_pack_type(MMDBTypeID.INT32, ">i"),
                MMDBTypeID.UINT64: self._encode_uint(MMDBTypeID.UINT64, 8),
                MMDBTypeID.UINT128: self._encode_uint(MMDBTypeID.UINT128, 16),
                MMDBTypeID.ARRAY: self._encode_array,
                MMDBTypeID.BOOLEAN: self._encode_boolean,
                MMDBTypeID.FLOAT: self._encode_pack_type(MMDBTypeID.FLOAT, ">f"),
            }
        return self._type_encoder

    def _make_header(self, type_id, length):
        if length >= 16843036:
            raise Exception("length >= 16843036")

        elif length >= 65821:
            five_bits = 31
            length -= 65821
            b3 = length & 0xFF
            b2 = (length >> 8) & 0xFF
            b1 = (length >> 16) & 0xFF
            additional_length_bytes = struct.pack(">BBB", b1, b2, b3)

        elif length >= 285:
            five_bits = 30
            length -= 285
            b2 = length & 0xFF
            b1 = (length >> 8) & 0xFF
            additional_length_bytes = struct.pack(">BB", b1, b2)

        elif length >= 29:
            five_bits = 29
            length -= 29
            additional_length_bytes = struct.pack(">B", length & 0xFF)

        else:
            five_bits = length
            additional_length_bytes = b""

        if type_id <= 7:
            res = struct.pack(">B", (type_id << 5) + five_bits)
        else:
            res = struct.pack(">BB", five_bits, type_id - 7)

        return res + additional_length_bytes

    def python_type_id(self, value):
        value_type = type(value)
        type_id = self._python_type_id.get(value_type)
        if type_id:
            return type_id
        if value_type is int:
            if self.int_type == "auto":
                if value > UINT64_MAX:
                    return MMDBTypeID.UINT128
                elif value > UINT32_MAX:
                    return MMDBTypeID.UINT64
                elif value > UINT16_MAX:
                    return MMDBTypeID.UINT32
                elif value < 0:
                    return MMDBTypeID.INT32
                else:
                    return MMDBTypeID.UINT16
            elif self.int_type in ("u16", "uint16", MmdbU16):
                return MMDBTypeID.UINT16
            elif self.int_type in ("u32", "uint32", MmdbU32):
                return MMDBTypeID.UINT32
            elif self.int_type in ("u64", "uint64", MmdbU64):
                return MMDBTypeID.UINT64
            elif self.int_type in ("u128", "uint128", MmdbU128):
                return MMDBTypeID.UINT128
            elif self.int_type in ("i32", "int32", MmdbI32):
                return MMDBTypeID.INT32
            else:
                raise ValueError(f"unknown int_type={self.int_type}")
        elif value_type is float:
            if self.float_type in ("f32", "float32", MmdbF32):
                return MMDBTypeID.FLOAT
            elif self.float_type in ("f64", "float64", MmdbF64):
                return MMDBTypeID.DOUBLE
            else:
                raise ValueError(f"unknown float_type={self.float_type}")
        elif value_type is Decimal:
            return MMDBTypeID.DOUBLE
        raise TypeError(f"unknown type {value_type}")

    def _freeze(self, value):
        if isinstance(value, dict):
            return tuple((k, self._freeze(v)) for k, v in value.items())
        elif isinstance(value, list):
            return tuple(self._freeze(v) for v in value)
        return value

    def encode_meta(self, meta):
        res = self._make_header(MMDBTypeID.MAP, len(meta))
        meta_type = {
            "node_count": 6,
            "record_size": 5,
            "ip_version": 5,
            "binary_format_major_version": 5,
            "binary_format_minor_version": 5,
            "build_epoch": 9,
        }
        for k, v in list(meta.items()):
            # Keys are always stored by value.
            res += self.encode(k)
            res += self.encode(v, meta_type.get(k))
        return res

    def encode(self, value, type_id=None, return_offset=False):
        if self.cache:
            cache_key = self._freeze(value)
            try:
                offset = self.data_cache[cache_key]
                return offset if return_offset else self._encode_pointer(offset)
            except KeyError:
                pass

        if not type_id:
            type_id = self.python_type_id(value)

        try:
            encoder = self.type_encoder[type_id]
        except KeyError as err:
            raise ValueError(f"unknown type_id={type_id}") from err

        if isinstance(value, MmdbBaseType):
            value = value.value
        res = encoder(value)

        if self.cache:
            self.data_list.append(res)
            offset = self.data_pointer
            self.data_pointer += len(res)
            self.data_cache[cache_key] = offset
            return offset if return_offset else self._encode_pointer(offset)
        return res


class TreeWriter:
    encoder_cls = Encoder

    def __init__(
        self,
        tree: "SearchTreeNode",
        meta: dict,
        int_type: IntType = "auto",
        float_type: FloatType = "f64",
    ):
        self._node_idx = {}
        self._leaf_offset = {}
        self._node_list = []
        self._node_counter = 0
        self._record_size = 0

        self.tree = tree
        self.meta = meta

        self.encoder = self.encoder_cls(
            cache=True, int_type=int_type, float_type=float_type
        )

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
            **self.meta,
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
            raise Exception("record_size > 32")

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
                offset = self.encoder.encode(node.value, return_offset=True)
                self._leaf_offset[node_id] = offset + 16
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
            b1 = (left_idx >> 16) & 0xFF
            b2 = (left_idx >> 8) & 0xFF
            b3 = left_idx & 0xFF
            b4 = (right_idx >> 16) & 0xFF
            b5 = (right_idx >> 8) & 0xFF
            b6 = right_idx & 0xFF
            return struct.pack(">BBBBBB", b1, b2, b3, b4, b5, b6)

        elif self.record_size == 28:
            b1 = (left_idx >> 16) & 0xFF
            b2 = (left_idx >> 8) & 0xFF
            b3 = left_idx & 0xFF
            b4 = ((left_idx >> 24) & 0xF) * 16 + ((right_idx >> 24) & 0xF)
            b5 = (right_idx >> 16) & 0xFF
            b6 = (right_idx >> 8) & 0xFF
            b7 = right_idx & 0xFF
            return struct.pack(">BBBBBBB", b1, b2, b3, b4, b5, b6, b7)

        elif self.record_size == 32:
            return struct.pack(">II", left_idx, right_idx)

        else:
            raise Exception("self.record_size > 32")

    def write(self, fname):
        self._enumerate_nodes(self.tree)
        self._adjust_record_size()

        with open(fname, "wb") as f:
            for node in self._node_list:
                f.write(self._cal_node_bytes(node))

            f.write(b"\x00" * 16)

            for element in self._data_list:
                f.write(element)

            f.write(METADATA_MAGIC)
            f.write(self.encoder_cls(cache=False).encode_meta(self._build_meta()))


def bits_rstrip(n, length=None, keep=0):
    return map(int, bin(n)[2:].rjust(length, "0")[:keep])


class MMDBWriter:
    def __init__(
        self,
        ip_version=4,
        database_type="GeoIP",
        languages: List[str] = None,
        description: Union[Dict[str, str], str] = "GeoIP db",
        ipv4_compatible=False,
        int_type: IntType = "auto",
        float_type: FloatType = "f64",
    ):
        """
        Args:
            ip_version: The IP version of the database. Defaults to 4.
            database_type: The type of the database. Defaults to "GeoIP".
            languages: A list of languages. Defaults to [].
            description: A description of the database for every language.
            ipv4_compatible: Whether the database is compatible with IPv4.
            int_type: The type of integer to use. Defaults to "auto".
            float_type: The type of float to use. Defaults to "f64".

        Note:
            If you want to store an IPv4 address in an IPv6 database, you should set
            ipv4_compatible=True.

            If you want to use a specific integer type, you can set int_type to
            "u16", "u32", "u64", "u128", or "i32".
        """
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
            raise ValueError(f"ip_version should be 4 or 6, {ip_version} is incorrect")
        if ip_version == 4 and ipv4_compatible:
            raise ValueError("ipv4_compatible=True can set when ip_version=6")
        if not self.binary_format_major_version:
            raise ValueError(
                f"major_version can't be empty or 0: {self.binary_format_major_version}"
            )
        if isinstance(description, str):
            self.description = {i: description for i in languages}
        for i in languages:
            if i not in self.description:
                raise ValueError("language {} must have description!")

        self.int_type = int_type
        self.float_type = float_type

    def insert_network(self, network: IPSet, content: MMDBType):
        """
        Inserts a network into the MaxMind database.

        Args:
           network: The network to be inserted. It should be an instance of
                    netaddr.IPSet.
           content: The content associated with the network. It can be a
                    dictionary, list, string, bytes, integer, or boolean.


        Raises:
           ValueError: If the network is not an instance of netaddr.IPSet.
           ValueError: If an IPv6 address is inserted into an IPv4-only database.
           ValueError: If an IPv4 address is inserted into an IPv6 database without
                       setting ipv4_compatible=True.

        Note:
           This method modifies the internal tree structure of the MMDBWriter instance.
        """
        leaf = SearchTreeLeaf(content)
        if not isinstance(network, IPSet):
            raise ValueError("network type should be netaddr.IPSet.")
        network = network.iter_cidrs()
        for cidr in network:
            if self.ip_version == 4 and cidr.version == 6:
                raise ValueError(
                    f"You inserted a IPv6 address {cidr} " "to an IPv4-only database."
                )
            if self.ip_version == 6 and cidr.version == 4:
                if not self.ipv4_compatible:
                    raise ValueError(
                        f"You inserted a IPv4 address {cidr} to an IPv6 database."
                        "Please use ipv4_compatible=True option store "
                        "IPv4 address in IPv6 database as ::/96 format"
                    )
                cidr = cidr.ipv6(True)
            node = self.tree
            bits = list(bits_rstrip(cidr.value, self._bit_length, cidr.prefixlen))
            current_node = node
            supernet_leaf = None  # Tracks whether we are inserting into a subnet
            for index, ip_bit in enumerate(bits[:-1]):
                previous_node = current_node
                current_node = previous_node.get_or_create(ip_bit)

                if isinstance(current_node, SearchTreeLeaf):
                    current_cidr = IPNetwork(
                        (
                            int(
                                "".join(map(str, bits[: index + 1])).ljust(
                                    self._bit_length, "0"
                                ),
                                2,
                            ),
                            index + 1,
                        )
                    )
                    logger.info(
                        f"Inserting {cidr} ({content}) into subnet of "
                        f"{current_cidr} ({current_node.value})"
                    )
                    supernet_leaf = current_node
                    current_node = SearchTreeNode()
                    previous_node[ip_bit] = current_node

                if supernet_leaf:
                    next_bit = bits[index + 1]
                    # Insert supernet information on each inverse bit of
                    # the current subnet
                    current_node[1 - next_bit] = supernet_leaf
            current_node[bits[-1]] = leaf

    def to_db_file(self, filename: str):
        return TreeWriter(
            self.tree, self._build_meta(), self.int_type, self.float_type
        ).write(filename)

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
