package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"github.com/oschwald/maxminddb-golang"
	"log"
	"math/big"
	"net"
	"os"
)

var (
	db = flag.String("db", "", "Path to the MaxMind DB file")
	ip = flag.String("ip", "", "IP address to look up")
)

type Record struct {
	I32    int            `json:"i32" maxminddb:"i32"`
	F32    float32        `json:"f32" maxminddb:"f32"`
	F64    float64        `json:"f64" maxminddb:"f64"`
	U16    uint16         `json:"u16" maxminddb:"u16"`
	U32    uint32         `json:"u32" maxminddb:"u32"`
	U64    uint64         `json:"u64" maxminddb:"u64"`
	U128   *big.Int       `json:"u128" maxminddb:"u128"`
	Array  []any          `json:"array" maxminddb:"array"`
	Map    map[string]any `json:"map" maxminddb:"map"`
	Bytes  []byte         `json:"bytes" maxminddb:"bytes"`
	String string         `json:"string" maxminddb:"string"`
	Bool   bool           `json:"bool" maxminddb:"bool"`
}

func main() {
	flag.Parse()
	if *db == "" || *ip == "" {
		flag.PrintDefaults()
		os.Exit(1)
	}
	db, err := maxminddb.Open(*db)
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close()

	ip := net.ParseIP(*ip)

	var record Record

	err = db.Lookup(ip, &record)
	if err != nil {
		log.Panic(err)
	}
	data, err := json.Marshal(record)
	if err != nil {
		log.Panic(err)
	}
	fmt.Println(string(data))
}
