- [MaxMind-DB-Writer-python](#maxmind-db-writer-python)
    * [Install](#install)
    * [Usage](#usage)
    * [Examples](#examples)
    * [Using the Java Client](#using-the-java-client)
        + [TLDR](#tldr)
        + [Underlying Principles](#underlying-principles)
    * [Type Enforcement](#type-enforcement)
    * [Reference:](#reference-)

# MaxMind-DB-Writer-python

Make `mmdb` format ip library file which can be read by [
`maxmind` official language reader](https://dev.maxmind.com/geoip/geoip2/downloadable/)

~~[The official perl writer](https://github.com/maxmind/MaxMind-DB-Writer-perl) was written in perl,
which was difficult to customize.
So I implemented the `MaxmindDB format` ip library in python language.~~

MaxMind has now released an official Go version of the MMDB writer.
If you prefer using Go, you can check out the official Go
implementation [mmdbwriter](https://github.com/maxmind/mmdbwriter).
This project still provides a Python alternative for those who need it.

## Install

```shell script
pip install -U mmdb_writer
```

## Usage

```python
from netaddr import IPSet

from mmdb_writer import MMDBWriter

writer = MMDBWriter()

writer.insert_network(IPSet(['1.1.0.0/24', '1.1.1.0/24']), {'country': 'COUNTRY', 'isp': 'ISP'})
writer.to_db_file('test.mmdb')

import maxminddb

m = maxminddb.open_database('test.mmdb')
r = m.get('1.1.1.1')
assert r == {'country': 'COUNTRY', 'isp': 'ISP'}
```

## Examples

see [csv_to_mmdb.py](./examples/csv_to_mmdb.py)
Here is a professional and clear translation of the README.md section from Chinese into English:

## Using the Java Client

If you are using the Java client, you need to be careful to set the `int_type` parameter so that Java correctly
recognizes the integer type in the MMDB file.

Example:

```python
from mmdb_writer import MMDBWriter

writer = MMDBWriter(int_type='i32')
```

Alternatively, you can explicitly specify data types using the [Type Enforcement](#type-enforcement) section.

### Underlying Principles

In Java, when deserializing to a structure, the numeric types will use the original MMDB numeric types. The specific
conversion relationships are as follows:

| mmdb type | java type  |
|-----------|------------|
| float     | Float      |
| double    | Double     |
| int32     | Integer    |
| uint16    | Integer    |
| uint32    | Long       |
| uint64    | BigInteger |
| uint128   | BigInteger |

When using the Python writer to generate an MMDB file, by default, it converts integers to the corresponding MMDB type
based on the size of the `int`. For instance, `int(1)` would convert to `uint16`, and `int(2**16+1)` would convert
to `uint32`. This may cause deserialization failures in Java clients. Therefore, it is necessary to specify
the `int_type` parameter when generating MMDB files to define the numeric type accurately.

## Type Enforcement

MMDB supports a variety of numeric types such as `int32`, `uint16`, `uint32`, `uint64`, `uint128` for integers,
and `f32`, `f64` for floating points, while Python only has one integer type and one float type (actually `f64`).

Therefore, when generating an MMDB file, you need to specify the `int_type` parameter to define the numeric type of the
MMDB file. The behaviors for different `int_type` settings are:

| int_type       | Behavior                                                                                                                                                                                                                                                      |
|----------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| auto (default) | Automatically selects the MMDB numeric type based on the value size. <br/>Rules: <br/>`int32` for value < 0 <br/>`uint16` for 0 <= value < 2^16<br/>`uint32` for 2^16 <= value < 2^32<br/>`uint64` for 2^32 <= value < 2^64<br/> `uint128` for value >= 2^64. |
| i32            | Stores all integer types as `int32`.                                                                                                                                                                                                                          |
| u16            | Stores all integer types as `uint16`.                                                                                                                                                                                                                         |
| u32            | Stores all integer types as `uint32`.                                                                                                                                                                                                                         |
| u64            | Stores all integer types as `uint64`.                                                                                                                                                                                                                         |
| u128           | Stores all integer types as `uint128`.                                                                                                                                                                                                                        |

If you want to use different int types for different scenarios, you can use type wrapping:

```python
from mmdb_writer import MMDBWriter, MmdbI32, MmdbF32

writer = MMDBWriter()
# the value of field "i32" will be stored as int32 type
writer.insert_network(IPSet(["1.0.0.0/24"]), {"i32": MmdbI32(128), "f32": MmdbF32(1.22)})
```

## Reference:

- [MaxmindDB format](http://maxmind.github.io/MaxMind-DB/)
- [geoip-mmdb](https://github.com/i-rinat/geoip-mmdb)
