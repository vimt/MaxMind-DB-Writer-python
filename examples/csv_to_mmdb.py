import csv
from collections import defaultdict

from netaddr import IPNetwork, IPSet

from mmdb_writer import MMDBWriter


def main():
    writer = MMDBWriter(
        4, "Test.GeoIP", languages=["EN"], description="Test IP library"
    )
    data = defaultdict(list)

    # merge cidr
    with open("fake_ip_info.csv") as f:
        reader = csv.DictReader(f)
        for line in reader:
            data[(line["country"], line["isp"])].append(
                IPNetwork(f'{line["ip"]}/{line["prefixlen"]}')
            )
    for index, cidrs in data.items():
        writer.insert_network(IPSet(cidrs), {"country": index[0], "isp": index[1]})
    writer.to_db_file("fake_ip_library.mmdb")


def test_read():
    import maxminddb

    m = maxminddb.open_database("fake_ip_library.mmdb")
    r = m.get("3.1.1.1")
    print(r)


if __name__ == "__main__":
    main()
