import base64
import json
import logging
import os
import subprocess
import unittest
from pathlib import Path

from netaddr.ip.sets import IPSet

from mmdb_writer import MMDBWriter, MmdbBaseType, MmdbF32
from tests.record import Record

logging.basicConfig(
    format="[%(asctime)s: %(levelname)s] %(message)s", level=logging.INFO
)
logger = logging.getLogger(__name__)

BASE_DIR = Path(__file__).parent.absolute()


def run(command: list):
    print(f"Running command: {command}")
    result = subprocess.run(command, check=True, stdout=subprocess.PIPE)
    return result.stdout


class TestClients(unittest.TestCase):
    def setUp(self) -> None:
        self.filepath = Path("_test.mmdb").absolute()
        self.filepath.unlink(True)
        self.ip = "1.1.1.1"
        self.origin_data = Record.random()
        self.generate_mmdb()
        self.maxDiff = None

    def tearDown(self) -> None:
        self.filepath.unlink(True)

    def generate_mmdb(self):
        ip_version = 4
        database_type = "test_client"
        languages = ["en"]
        description = {"en": "for testing purposes only"}
        writer = MMDBWriter(
            ip_version=ip_version,
            database_type=database_type,
            languages=languages,
            description=description,
            ipv4_compatible=False,
        )

        writer.insert_network(IPSet(["1.0.0.0/8"]), self.origin_data.dict())

        # insert other useless record
        for i in range(2, 250):
            info = Record.random()
            writer.insert_network(IPSet([f"{i}.0.0.0/8"]), info.dict())

        writer.to_db_file(str(self.filepath))

    def convert_bytes(self, d, bytes_convert):
        if isinstance(d, bytes):
            return bytes_convert(d)
        elif isinstance(d, dict):
            return {k: self.convert_bytes(v, bytes_convert) for k, v in d.items()}
        elif isinstance(d, list):
            return [self.convert_bytes(i, bytes_convert) for i in d]
        elif isinstance(d, MmdbF32):
            # convert float32 to float
            return float(str(d.value))
        elif isinstance(d, MmdbBaseType):
            return d.value
        else:
            return d

    def test_java(self):
        java_dir = BASE_DIR / "clients" / "java"
        self.assertTrue(java_dir.exists())
        os.chdir(java_dir)
        run(["mvn", "clean", "package"])
        java_data_str = run(
            [
                "java",
                "-jar",
                "target/mmdb-test-jar-with-dependencies.jar",
                "-db",
                str(self.filepath),
                "-ip",
                self.ip,
            ]
        )
        java_data = json.loads(java_data_str)
        should_data = self.origin_data.dict()

        # java bytes marshal as i8 list
        should_data = self.convert_bytes(
            should_data, lambda x: [i if i <= 127 else i - 256 for i in x]
        )
        self.assertDictEqual(should_data, java_data)

    def test_go(self):
        go_dir = BASE_DIR / "clients" / "go"
        self.assertTrue(go_dir.exists())
        os.chdir(go_dir)
        go_data_str = run(
            ["go", "run", "main.go", "-db", str(self.filepath), "-ip", self.ip]
        )
        go_data = json.loads(go_data_str)

        should_data = self.origin_data.dict()
        # go bytes marshal as base64 str
        should_data = self.convert_bytes(
            should_data, lambda x: base64.b64encode(x).decode()
        )
        self.assertDictEqual(should_data, go_data)
