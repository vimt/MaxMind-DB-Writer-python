import com.google.gson.Gson;
import com.maxmind.db.MaxMindDbConstructor;
import com.maxmind.db.MaxMindDbParameter;
import com.maxmind.db.Reader;
import org.kohsuke.args4j.CmdLineParser;
import org.kohsuke.args4j.Option;

import java.io.File;
import java.io.IOException;
import java.math.BigInteger;
import java.net.InetAddress;
import java.util.List;
import java.util.Map;

public class Main {
    @Option(name = "-db", usage = "Path to the MMDB file", required = true)
    private String databasePath;

    @Option(name = "-ip", usage = "IP address to lookup", required = true)
    private String ipAddress;

    public static void main(String[] args) throws Exception {
        Main lookup = new Main();
        CmdLineParser parser = new CmdLineParser(lookup);
        parser.parseArgument(args);

        lookup.run();
    }

    public void run() throws IOException {
        File database = new File(databasePath);
        Gson gson = new Gson();

        try (Reader reader = new Reader(database)) {
            InetAddress address = InetAddress.getByName(ipAddress);

            Record result = reader.get(address, Record.class);
            String jsonResult = gson.toJson(result);
            System.out.println(jsonResult);
        }
    }


    public static class Record {
        private Integer i32;
        private Float f32;
        private Double f64;
        private Integer u16;
        private Long u32;
        private BigInteger u64;
        private BigInteger u128;
        private List<Object> array;
        private Map<String, Object> map;
        private byte[] bytes;
        private String string;
        private Boolean bool;

        @MaxMindDbConstructor
        public Record(
                @MaxMindDbParameter(name = "i32") Integer i32,
                @MaxMindDbParameter(name = "f32") Float f32,
                @MaxMindDbParameter(name = "f64") Double f64,
                @MaxMindDbParameter(name = "u16") Integer u16,
                @MaxMindDbParameter(name = "u32") Long u32,
                @MaxMindDbParameter(name = "u64") BigInteger u64,
                @MaxMindDbParameter(name = "u128") BigInteger u128,
                @MaxMindDbParameter(name = "array") List<Object> array,
                @MaxMindDbParameter(name = "map") Map<String, Object> map,
                @MaxMindDbParameter(name = "bytes") byte[] bytes,
                @MaxMindDbParameter(name = "string") String string,
                @MaxMindDbParameter(name = "bool") Boolean bool
        ) {
            this.i32 = i32;
            this.f32 = f32;
            this.f64 = f64;
            this.u16 = u16;
            this.u32 = u32;
            this.u64 = u64;
            this.u128 = u128;
            this.array = array;
            this.map = map;
            this.bytes = bytes;
            this.string = string;
            this.bool = bool;
        }
    }
}
