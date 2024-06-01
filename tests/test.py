# coding: utf-8
import logging
import os.path
import unittest

import maxminddb
from netaddr import IPSet

from mmdb_writer import MMDBWriter

logging.basicConfig(
    format="[%(asctime)s: %(levelname)s] %(message)s", level=logging.INFO
)
info1 = {"country": "c1", "isp": "ISP1"}
info2 = {"country": "c2", "isp": "ISP2"}


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

    def test_encode_type(self):
        writer = MMDBWriter()
        info = {
            "int": 1,
            "float": 1.0 / 3,
            "list": ["a", "b", "c"],
            "dict": {"k": "v"},
            "bytes": b"bytes",
            "str": "str",
        }
        writer.insert_network(IPSet(["1.1.0.0/24"]), info)
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            get = m.get("1.1.0.255")
            self.assertEqual(len(info), len(get), mode)
            self.assertEqual(info["int"], get["int"], mode)
            self.assertTrue(abs(info["float"] - get["float"]) < 1e-5, mode)
            self.assertEqual(info["list"], get["list"], mode)
            self.assertEqual(info["dict"], get["dict"], mode)
            self.assertEqual(info["bytes"], get["bytes"], mode)
            self.assertEqual(info["str"], get["str"], mode)
            m.close()

    def test_4in6(self):
        writer = MMDBWriter(ip_version=6, ipv4_compatible=True)
        writer.insert_network(IPSet(["1.1.0.0/24"]), info1)
        writer.insert_network(IPSet(["fe80::/16"]), info2)
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            self.assertEqual(info1, m.get("1.1.0.1"), mode)
            self.assertEqual(info2, m.get("fe80::1"), mode)
            m.close()

    def test_insert_subnet(self):
        writer = MMDBWriter()
        writer.insert_network(IPSet(["1.0.0.0/8"]), info1)
        writer.insert_network(IPSet(["1.10.10.0/24"]), info2)
        writer.to_db_file(self.filename)
        for mode in (maxminddb.MODE_MMAP_EXT, maxminddb.MODE_MMAP, maxminddb.MODE_FILE):
            m = maxminddb.open_database(self.filename, mode=mode)
            self.assertEqual(info1, m.get("1.1.0.1"), mode)
            self.assertEqual(info1, m.get("1.10.0.1"), mode)
            self.assertEqual(info2, m.get("1.10.10.1"), mode)
            m.close()
