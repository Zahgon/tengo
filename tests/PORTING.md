# Where the Go tests went

The Go test suite was deleted with the Go sources. Its scripts and expected values are carried over
here, taken from running the Go tests at the migration's base commit, so these tests assert what
the Go build does, not what this port happens to do. Tests of the Go embedding API have no Rust
counterpart, because this port is a command and not a Go library.

| Go test file | Rust test file | Carried over |
| :-- | :-- | :-- |
| `vm_test.go` | `tests/vm_1.rs`, `tests/vm_2.rs` | Every script, in both passes (direct and as an imported module), except the cases below |
| `compiler_test.go` | `tests/compiler.rs` | Instructions and constants for every source, and every compile error |
| `parser/parser_test.go`, `parser/ast_test.go` | `tests/parser.rs` | Every source, through the String form of Go's nodes, and every parse error |
| `parser/scanner_test.go` | `tests/parser.rs` | The token table with line and column, without comment tokens |
| `symbol_table_test.go` | `tests/symbol_table.rs` | Define and resolve across scopes and blocks |
| `objects_test.go` | `tests/objects.rs` | Type names, truthiness, string forms, binary operators, equality |
| `builtins_test.go` | `tests/builtins.rs` | The delete, splice and range tables and the freeze cases, called from a script |
| `stdlib/stdlib_test.go`, `text_test.go`, `text_regexp_test.go`, `json_test.go`, `times_test.go`, `base64_test.go`, `hex_test.go`, `fmt_test.go` | `tests/stdlib.rs` | Each module call, made from a script |

## Not carried over

| Go tests | Why |
| :-- | :-- |
| `script_test.go`, `variable_test.go`, `eval_test.go`, `example_test.go`, `tengo_test.go` | The Go embedding API (`Script`, `Variable`, `Eval`, `ToObject` and friends), which has no Rust counterpart |
| `bytecode_test.go` | Gob encoding of compiled bytecode; this port does not write bytecode files |
| `stdlib/func_typedefs_test.go`, `stdlib/json/json_test.go` | Adapters from Go functions and the Go JSON package API |
| `TestCompiler_custom_extension`, `TestCompilerNewCompiler_default_file_extension`, `TestCompilerSetImportExt_extension_name_validation`, `TestCompiler_ReplaceBuiltinModule`, `TestCompiler_ConcurrentParallelExecution_*` | Compiler options and Go concurrency of the embedding API |
| `TestParserError`, `TestParserErrorList`, `TestAllModuleNames`, `TestGetModules`, `TestFromInterface_JSONNumber`, `TestScript_JSONNumber` | Go types and helpers of the embedding API |
| `stdlib/os_test.go`, `stdlib/rand_test.go` and their cases in `stdlib_test.go` | This port has no `os` or `rand` module |
| `TestStripCR` | Comment tokens: this port's scanner has no `ScanComments` mode |
| 70 VM cases | They register Go callbacks or custom Go types, or set an allocation limit, none of which a script can express |
| 11 cases of `TestReplaceLimit`, `TestTextRepeat`, `TestBytesN`, `TestRangeN`, `TestString` | They lower package-level size limits, which are constants here |
| 2 cases of `TestTimes` | `Pacific/Auckland`: this port has no time zone database, a known gap from the Go build |
