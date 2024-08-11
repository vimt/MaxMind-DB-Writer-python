
1. 把 rust 的部分单独列出来，这样可以让 rust 也成为一个单独的库
   - 是不是应该拆成两个 crate
   - 先不搞。
2. readme 要给出 2.0 和 1.0 之间的差异
3. 写一个 benchmark ，看一下 2.0 和 1.0 的性能差异
4. ai 生成一下 rust doc
5. ci，怎么发布
6. 怎么错误处理

功能
1. 从 mmdb 反解出 csv

need try
1. 输入类型是 hashmap<string, string> ，在 python 里，必须输入 {"a": "b"} 吗？ {b"a": "b"} 行不行？ 数字行不行
2. 单独创建 几个 Error，用 this error