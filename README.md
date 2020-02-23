# MaxMind-DB-Writer-python

Make `mmdb` format ip library file which can be read by [`maxmind` official language reader](https://dev.maxmind.com/geoip/geoip2/downloadable/)

[The official perl writer](https://github.com/maxmind/MaxMind-DB-Writer-perl) was written in perl, which was difficult to customize. So I implemented the `MaxmindDB format` ip library in python language
## Install
```shell script
pip install -U git+https://github.com/VimT/MaxMind-DB-Writer-python
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


## Reference: 
- [MaxmindDB format](http://maxmind.github.io/MaxMind-DB/)
- [geoip-mmdb](https://github.com/i-rinat/geoip-mmdb)
