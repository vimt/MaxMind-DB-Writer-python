# coding: utf-8
import logging
import os.path
import random
import struct
import unittest

import maxminddb
from netaddr import IPSet

from mmdb_writer import MMDBWriter
from tests.record import Record

logging.basicConfig(
    format="[%(asctime)s: %(levelname)s] %(message)s", level=logging.INFO
)
record1 = {"country": "c1", "isp": "ISP1"}
record2 = {"country": "c2", "isp": "ISP2"}


class TestBuild(unittest.TestCase):
    def setUp(self) -> None:
        self.filename = "_test.mmdb"

    def tearDown(self) -> None:
        if os.path.exists(self.filename):
            os.remove(self.filename)

    def test_metadata(self):
        ip_version = 6
        database_type = "test_database_type"
        languages = ["en", "ch"]
        description = {"en": "en test", "ch": "ch test"}
        writer = MMDBWriter(
            ip_version=ip_version,
            database_type=database_type,
            languages=languages,
            description=description,
            ipv4_compatible=False,
        )
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            self.assertEqual(ip_version, m.metadata().ip_version, mode)
            self.assertEqual(database_type, m.metadata().database_type, mode)
            self.assertEqual(languages, m.metadata().languages, mode)
            self.assertEqual(description, m.metadata().description, mode)
            m.close()

    def test_4in6(self):
        writer = MMDBWriter(ip_version=6, ipv4_compatible=True)
        writer.insert_network(IPSet(["1.1.0.0/24"]), record1)
        writer.insert_network(IPSet(["fe80::/16"]), record2)
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            self.assertEqual(record1, m.get("1.1.0.1"), mode)
            self.assertEqual(record2, m.get("fe80::1"), mode)
            m.close()

    def test_insert_subnet(self):
        writer = MMDBWriter()
        writer.insert_network(IPSet(["1.0.0.0/8"]), record1)
        writer.insert_network(IPSet(["1.10.10.0/24"]), record2)
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            self.assertEqual(record1, m.get("1.1.0.1"), mode)
            self.assertEqual(record1, m.get("1.10.0.1"), mode)
            self.assertEqual(record2, m.get("1.10.10.1"), mode)
            m.close()

    def test_int_type(self):
        value_range_map = {
            "i32": (-(2 ** 31), 2 ** 31 - 1),
            "u16": (0, 2 ** 16 - 1),
            "u32": (0, 2 ** 32 - 1),
            "u64": (0, 2 ** 64 - 1),
            "u128": (0, 2 ** 128 - 1),
        }
        for int_type in ("i32", "u16", "u32", "u64", "u128"):
            writer = MMDBWriter(int_type=int_type)

            (start, end) = value_range_map[int_type]
            ok_value = random.randint(start, end)
            bad_value1 = random.randint(end + 1, end + 2 ** 16)
            bad_value2 = random.randint(start - 2 ** 16, start - 1)
            writer.insert_network(IPSet(["1.0.0.0/8"]), {"value": ok_value})
            writer.to_db_file(self.filename)
            for bad_value in (bad_value1, bad_value2):
                writer.insert_network(IPSet(["1.0.0.0/8"]), {"value": bad_value})
                with self.assertRaises((ValueError, struct.error)):
                    writer.to_db_file(self.filename)
