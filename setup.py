# coding: utf-8
import os

from setuptools import setup

from mmdb_writer import __version__

f = open(os.path.join(os.path.dirname(__file__), 'README.md'))
readme = f.read()
f.close()

setup(
    name="mmdb_writer",
    version=__version__,
    description="Make `mmdb` format ip library file which can be read by maxmind official language reader",
    long_description=readme,
    long_description_content_type="text/markdown",
    author='VimT',
    author_email='me@vimt.me',
    url='https://github.com/VimT/MaxMind-DB-Writer-python',
    py_modules=['mmdb_writer'],
    platforms=['any'],
    zip_safe=False,
    python_requires=">=3.6",
    install_requires=['netaddr>=0.7'],
    tests_require=['maxminddb>=1.5'],
    classifiers=[
        'Development Status :: 5 - Production/Stable',
        'Programming Language :: Python',
        'Programming Language :: Python :: 3',
        'Programming Language :: Python :: 3.6',
        'Programming Language :: Python :: 3.7',
        'Programming Language :: Python :: 3.8',
        'Programming Language :: Python :: Implementation :: CPython',
    ],
)
