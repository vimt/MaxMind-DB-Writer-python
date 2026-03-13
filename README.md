- [MaxMind-DB-Writer-python](#maxmind-db-writer-python)
    * [Install](#install)
    * [Usage](#usage)
    * [Batch Insert](#batch-insert)
    * [Performance](#performance)
    * [Migration from v1 (Python) to v2 (Rust)](#migration-from-v1-python-to-v2-rust)
    * [Using the Java Client](#using-the-java-client)
    * [Type Enforcement](#type-enforcement)
    * [Reference](#reference)

# MaxMind-DB-Writer-python

Make `mmdb` format ip library file which can be read by
[`maxmind` official language reader](https://dev.maxmind.com/geoip/geoip2/downloadable/)

> **v2 is a Rust rewrite** via [PyO3](https://pyo3.rs/) and [maturin](https://github.com/PyO3/maturin).
> It is a drop-in replacement for v1 with a simplified API and significant performance improvements.
> See [Migration from v1 to v2](#migration-from-v1-python-to-v2-rust) if you are upgrading.

MaxMind has now released an official Go version of the MMDB writer.
If you prefer using Go, you can check out the official Go
implementation [mmdbwriter](https://github.com/maxmind/mmdbwriter).
This project still provides a Python alternative for those who need it.

## Install

```shell
pip install -U mmdb_writer
```

## Usage

```python
from mmdb_writer import MmdbWriter

writer = MmdbWriter()

writer.insert_network('1.1.0.0/24', {'country': 'COUNTRY', 'isp': 'ISP'})
writer.insert_network('1.1.1.0/24', {'country': 'COUNTRY', 'isp': 'ISP'})
writer.to_file('test.mmdb')

import maxminddb

m = maxminddb.open_database('test.mmdb')
r = m.get('1.1.1.1')
assert r == {'country': 'COUNTRY', 'isp': 'ISP'}
```

## Batch Insert

For large datasets, use `insert_networks()` to pass all records at once.
This avoids repeated Python→Rust boundary crossings and is ~8x faster than
calling `insert_network()` in a loop:

```python
from mmdb_writer import MmdbWriter

writer = MmdbWriter()

records = [
    ('1.0.0.0/8',  {'country': 'US'}),
    ('2.0.0.0/8',  {'country': 'DE'}),
    ('3.0.0.0/8',  {'country': 'CN'}),
    # ...
]
writer.insert_networks(records)
writer.to_file('output.mmdb')
```

## Performance

Benchmark on 1M and 4M `/24` networks (Intel Core i7, Linux, release build):

| Scale | Method | Insert | Build | Total | vs Python v1 |
|-------|--------|--------|-------|-------|-------------|
| 1M | Python v1 | 6.8s | 7.5s | 14.3s | — |
| 1M | Rust v2 single | 1.0s | 1.2s | 2.2s | **6.4×** |
| 1M | Rust v2 batch | 0.5s | 1.2s | 1.8s | **8.1×** |
| 4M | Python v1 | 27.6s | 31.2s | 58.8s | — |
| 4M | Rust v2 single | 3.6s | 5.1s | 8.8s | **6.7×** |
| 4M | Rust v2 batch | 2.2s | 5.1s | 7.3s | **8.1×** |

## Migration from v1 (Python) to v2 (Rust)

v2 simplifies the API: networks are now plain CIDR strings instead of `netaddr.IPSet` objects,
and the class is renamed from `MMDBWriter` to `MmdbWriter`.

| | v1 (Python) | v2 (Rust) |
|---|---|---|
| Import | `from mmdb_writer import MMDBWriter` | `from mmdb_writer import MmdbWriter` |
| Insert | `writer.insert_network(IPSet(['1.0.0.0/8']), data)` | `writer.insert_network('1.0.0.0/8', data)` |
| Batch insert | — | `writer.insert_networks([('1.0.0.0/8', data), ...])` |
| Write | `writer.to_db_file('out.mmdb')` | `writer.to_file('out.mmdb')` |
| Dependency | `netaddr` | none |

Type wrapper classes (`MmdbI32`, `MmdbU16`, `MmdbU32`, `MmdbU64`, `MmdbU128`) are
**passed as the `int_type` constructor argument**, not wrapped around values:

```python
# v1
writer.insert_network(IPSet(['1.0.0.0/8']), {'id': MmdbI32(42)})

# v2  — specify int_type once at writer construction
writer = MmdbWriter(int_type='i32')
writer.insert_network('1.0.0.0/8', {'id': 42})

# or pass the class itself as int_type
from mmdb_writer import MmdbWriter, MmdbI32
writer = MmdbWriter(int_type=MmdbI32)
```

## Using the Java Client

If you are using the Java client, set the `int_type` parameter so that Java correctly
recognizes the integer type in the MMDB file.

```python
from mmdb_writer import MmdbWriter

writer = MmdbWriter(int_type='i32')
```

### MMDB type → Java type mapping

| mmdb type | java type  |
|-----------|------------|
| float     | Float      |
| double    | Double     |
| int32     | Integer    |
| uint16    | Integer    |
| uint32    | Long       |
| uint64    | BigInteger |
| uint128   | BigInteger |

By default, Python integers are auto-mapped to the smallest fitting MMDB unsigned type.
For example, `1` → `uint16`, `2**17` → `uint32`. Specify `int_type` to override this.

## Type Enforcement

| int_type | Behavior |
|----------|----------|
| `auto` (default) | `int32` if < 0; `uint16` if < 2¹⁶; `uint32` if < 2³²; `uint64` if < 2⁶⁴; `uint128` otherwise |
| `i32` / `int32` | Always `int32` |
| `u16` / `uint16` | Always `uint16` |
| `u32` / `uint32` | Always `uint32` |
| `u64` / `uint64` | Always `uint64` |
| `u128` / `uint128` | Always `uint128` |

The type class itself (e.g. `MmdbI32`) can also be passed as `int_type`:

```python
from mmdb_writer import MmdbWriter, MmdbI32, MmdbU64

writer = MmdbWriter(int_type=MmdbI32)
writer.insert_network('1.0.0.0/8', {'score': -5, 'rank': 100})
```

## Reference

- [MaxmindDB format](http://maxmind.github.io/MaxMind-DB/)
- [geoip-mmdb](https://github.com/i-rinat/geoip-mmdb)
