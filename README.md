# eclap

Extended [clap](https://crates.io/crates/clap): namespaced config types that work with
serde and clap, without a `clap` dependency in your crate.

`#[derive(eclap::Parser)]` makes a root type with `parse`, `try_parse`, `parse_from` and
`try_parse_from`. `#[derive(eclap::Args)]` makes a reusable config type (a struct, or an
enum selected by a `type` flag) that has no names of its own: the field that holds it
decides the flag names and environment variables.

```toml
[dependencies]
eclap = { git = "https://github.com/mipsel64/eclap-rs" }
```

```rust
#[derive(eclap::Parser)]
struct Cli {
    #[eclap(ns = "db")]
    db: Db,
    #[arg(short, env)]
    verbose: bool,
}

#[derive(eclap::Args)]
struct Db {
    host: String,
    #[eclap(nested)]
    auth: Auth,
}

#[derive(eclap::Args)]
enum Auth {
    Anonymous,
    Password(Password),
}

#[derive(eclap::Args)]
struct Password {
    user: String,
}
```

This gives the flags `--db.host`, `--db.auth.type` (env `DB__AUTH__TYPE`) and
`--db.auth.password.user`, which is required only when `--db.auth.type Password` is set.

The naming rules and all attributes are in the [crate documentation](eclap/src/lib.rs).

## License

[MIT](LICENSE)
