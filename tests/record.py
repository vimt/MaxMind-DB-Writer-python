import random
from dataclasses import dataclass

import numpy as np

from mmdb_writer import MmdbF32, MmdbF64, MmdbI32, MmdbU16, MmdbU32, MmdbU64, MmdbU128


def random_str(length=10):
    return "".join(random.choices("abc中文", k=length))


def random_bytes(length=10):
    return bytes(random.choices(range(256), k=length))


def random_i32():
    return MmdbI32(random.randint(-(2**31), 0))


def random_f32():
    return MmdbF32(np.float32(random.random()))


def random_f64():
    return MmdbF64(random.random() * 1e128)


def random_u16():
    return MmdbU16(random.randint(0, 2**16 - 1))


def random_u32():
    return MmdbU32(random.randint(2**16, 2**32 - 1))


def random_u64():
    return MmdbU64(random.randint(2**32, 2**64 - 1))


def random_u128():
    return MmdbU128(random.randint(2**64, 2**128 - 1))


def random_array(length=10, nested_type=False):
    return [random_any(nested_type) for _ in range(length)]


def random_map(length=10, nested_type=False):
    return {random_str(): random_any(nested_type) for _ in range(length)}


def random_bool():
    return random.choice([True, False])


def random_any(nested_type=False):
    return random.choice(
        [
            random_i32,
            random_f32,
            random_f64,
            random_u16,
            random_u32,
            random_u64,
            random_u128,
            random_bytes,
            random_str,
            random_bool,
            *([random_array, random_map] if nested_type else []),
        ]
    )()


@dataclass
class Record:
    i32: MmdbI32
    f32: MmdbF32
    f64: MmdbF64
    u16: MmdbU16
    u32: MmdbU32
    u64: MmdbU64
    u128: MmdbU128
    array: list
    map: dict
    bytes: bytes
    string: str
    bool: bool

    @staticmethod
    def random():
        return Record(
            i32=random_i32(),
            f32=random_f32(),
            f64=random_f64(),
            u16=random_u16(),
            u32=random_u32(),
            u64=random_u64(),
            u128=random_u128(),
            array=random_array(5, True),
            map=random_map(5, True),
            bytes=random_bytes(),
            string=random_str(),
            bool=random_bool(),
        )

    def dict(self):
        return self.__dict__
