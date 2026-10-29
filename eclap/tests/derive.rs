use eclap::clap;
use eclap::clap::error::ErrorKind;
use eclap::clap::{CommandFactory, Parser as _};
use std::path::PathBuf;

#[derive(Debug, PartialEq, eclap::Args)]
struct Config {
    /// Key name registered in the coordinator.
    ///
    /// Must already exist.
    key: String,
    /// Print more.
    verbose: bool,
    #[eclap(nested)]
    signature: SignatureMethod,
}

/// How requests are signed.
#[derive(Debug, PartialEq, eclap::Args)]
enum SignatureMethod {
    CloudKMS(CloudKmsConfig),
    PrivateKey(PrivateKeyConfig),
}

#[derive(Debug, PartialEq, eclap::Args)]
struct CloudKmsConfig {
    project_id: Option<String>,
    key_ring: String,
    key_name: String,
}

#[derive(Debug, PartialEq, eclap::Args)]
struct PrivateKeyConfig {
    /// Path of the key file.
    path: PathBuf,
}

#[derive(Debug, clap::Args, PartialEq)]
struct Metrics {
    #[arg(long, default_value = "0.0.0.0:9000")]
    metrics_addr: String,
}

#[derive(Debug, clap::Subcommand, PartialEq)]
enum Cmd {
    Run {
        #[arg(long)]
        fast: bool,
    },
    Check,
}

#[derive(Clone, Debug, PartialEq, clap::ValueEnum)]
enum Level {
    Info,
    Debug,
}

/// My service.
///
/// Longer description.
#[derive(Debug, eclap::Parser)]
#[command(version)]
struct Cli {
    #[eclap(ns = "signer")]
    signer: Config,
    #[eclap(ns = "backup", ns_sep = '-')]
    backup: Config,
    #[eclap(nested)]
    log: LogConfig,
    #[eclap(ns = "")]
    common: Common,
    /// Ordinary argument declared after the namespaces.
    #[arg(short)]
    verbose: bool,
    #[arg(long, default_value_t = 8080)]
    port: u16,
    #[arg(long, default_value_t = 4_000_000_000)]
    limit: u64,
    #[arg(short, num_args = 1)]
    tag: Vec<String>,
    #[arg(value_enum)]
    level: Option<Level>,
    #[eclap(flatten)]
    metrics: Metrics,
    #[eclap(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Debug, eclap::Args)]
struct LogConfig {
    level: Option<String>,
}

#[derive(Debug, eclap::Args)]
struct Common {
    dry_run: bool,
}

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(["bin"].into_iter().chain(args.iter().copied()))
}

const SIGNER_KMS: [&str; 10] = [
    "--signer.key",
    "k",
    "--signer.signature.type",
    "CloudKMS",
    "--signer.signature.cloud-kms.key-ring",
    "r",
    "--signer.signature.cloud-kms.key-name",
    "n",
    "--backup-key",
    "b",
];

fn backup_pk(path: &str) -> Vec<&str> {
    vec![
        "--backup-signature-type",
        "PrivateKey",
        "--backup-signature-private-key-path",
        path,
    ]
}

fn full(extra: &[&str]) -> Result<Cli, clap::Error> {
    let args = [&SIGNER_KMS[..], &backup_pk("/k"), extra].concat();
    parse(&args)
}

#[test]
fn namespaces_separators_and_variants() {
    let cli = full(&["--log.level", "debug", "--dry-run"]).unwrap();
    assert_eq!(
        cli.signer,
        Config {
            key: "k".into(),
            verbose: false,
            signature: SignatureMethod::CloudKMS(CloudKmsConfig {
                project_id: None,
                key_ring: "r".into(),
                key_name: "n".into(),
            }),
        }
    );
    assert_eq!(
        cli.backup.signature,
        SignatureMethod::PrivateKey(PrivateKeyConfig { path: "/k".into() })
    );
    assert_eq!(cli.log.level.as_deref(), Some("debug"));
    assert!(cli.common.dry_run);
    assert_eq!(cli.port, 8080);
    assert_eq!(cli.limit, 4_000_000_000);
    assert_eq!(cli.metrics.metrics_addr, "0.0.0.0:9000");
    assert_eq!(cli.cmd, None);
}

#[test]
fn sibling_fields_of_one_enum_parse_independently() {
    let cli = full(&[
        "--signer.verbose",
        "--signer.signature.cloud-kms.project-id",
        "p",
    ])
    .unwrap();
    assert!(cli.signer.verbose && !cli.backup.verbose);
    let SignatureMethod::CloudKMS(kms) = cli.signer.signature else {
        panic!("wrong variant");
    };
    assert_eq!(kms.project_id.as_deref(), Some("p"));
    assert!(matches!(
        cli.backup.signature,
        SignatureMethod::PrivateKey(_)
    ));
}

#[test]
fn missing_selected_variant_field_is_an_error() {
    let args = [
        "--signer.key",
        "k",
        "--signer.signature.type",
        "CloudKMS",
        "--signer.signature.cloud-kms.key-ring",
        "r",
        "--backup-key",
        "b",
    ];
    let args = [&args[..], &backup_pk("/k")].concat();
    let err = parse(&args).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    assert!(
        err.to_string()
            .contains("--signer.signature.cloud-kms.key-name")
    );
}

#[test]
fn other_variant_fields_are_not_required() {
    // backup selects PrivateKey, so none of its cloud-kms fields is required.
    assert!(full(&[]).is_ok());
}

#[test]
fn missing_tag_and_invalid_tag_are_errors() {
    let err = parse(&["--signer.key", "k"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    assert!(err.to_string().contains("--signer.signature.type"));

    let err = parse(&["--signer.key", "k", "--signer.signature.type", "Nope"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidValue);
}

#[test]
fn command_name_about_and_version() {
    let cmd = Cli::command();
    assert_eq!(cmd.get_name(), "eclap");
    assert_eq!(cmd.get_about().unwrap().to_string(), "My service.");
    assert!(
        cmd.get_long_about()
            .unwrap()
            .to_string()
            .contains("Longer description.")
    );
    let err = parse(&["--version"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    assert!(err.to_string().contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn arg_short_default_and_forwarding() {
    let cmd = Cli::command();
    let verbose = cmd
        .get_arguments()
        .find(|a| a.get_id() == "verbose")
        .unwrap();
    assert_eq!(verbose.get_short(), Some('v'));
    let port = cmd.get_arguments().find(|a| a.get_id() == "port").unwrap();
    assert!(!port.is_required_set());

    let cli = full(&[
        "-v", "--port", "1", "-t", "a", "-t", "b", "--level", "debug",
    ])
    .unwrap();
    assert!(cli.verbose);
    assert_eq!(cli.port, 1);
    assert_eq!(cli.tag, ["a", "b"]);
    assert_eq!(cli.level, Some(Level::Debug));
}

#[test]
fn flatten_and_subcommand() {
    let cli = full(&["--metrics-addr", "x", "run", "--fast"]).unwrap();
    assert_eq!(cli.metrics.metrics_addr, "x");
    assert_eq!(cli.cmd, Some(Cmd::Run { fast: true }));
    assert_eq!(full(&["check"]).unwrap().cmd, Some(Cmd::Check));
}

#[derive(Debug, eclap::Parser)]
struct Needs {
    #[eclap(subcommand)]
    cmd: Cmd,
}

#[test]
fn required_subcommand() {
    let err = Needs::try_parse_from(["bin"]).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    assert_eq!(
        Needs::try_parse_from(["bin", "check"]).unwrap().cmd,
        Cmd::Check
    );
}

#[derive(Debug, PartialEq, eclap::Args)]
struct Tls {
    cert: String,
}

#[derive(Debug, PartialEq, eclap::Args)]
enum Mode {
    Off,
    Remote(Remote),
}

#[derive(Debug, PartialEq, eclap::Args)]
struct Remote {
    #[eclap(nested)]
    auth: Auth,
}

#[derive(Debug, PartialEq, eclap::Args)]
enum Auth {
    Token(Token),
    Anonymous,
}

#[derive(Debug, PartialEq, eclap::Args)]
struct Token {
    value: String,
}

#[derive(Debug, eclap::Parser)]
struct OuterCli {
    #[eclap(ns = "ns")]
    outer: Outer,
    #[eclap(ns = "app")]
    app: App,
}

#[derive(Debug, PartialEq, eclap::Args)]
struct Outer {
    #[eclap(nested)]
    tls: Tls,
    #[eclap(nested)]
    mode: Mode,
}

#[derive(Debug, PartialEq, eclap::Args)]
struct App {
    #[eclap(ns = "db", ns_sep = '-')]
    db: Db,
}

#[derive(Debug, PartialEq, eclap::Args)]
struct Db {
    host: String,
}

fn parse_outer(args: &[&str]) -> Result<OuterCli, clap::Error> {
    let base = ["bin", "--app.db-host", "h"];
    OuterCli::try_parse_from(base.into_iter().chain(args.iter().copied()))
}

#[test]
fn nested_names_and_separator_join() {
    let cli = parse_outer(&["--ns.tls.cert", "c", "--ns.mode.type", "Off"]).unwrap();
    assert_eq!(cli.outer.tls, Tls { cert: "c".into() });
    assert_eq!(cli.outer.mode, Mode::Off);
    assert_eq!(cli.app.db.host, "h");
}

#[test]
fn enum_inside_variant() {
    let cli = parse_outer(&[
        "--ns.tls.cert",
        "c",
        "--ns.mode.type",
        "Remote",
        "--ns.mode.remote.auth.type",
        "Token",
        "--ns.mode.remote.auth.token.value",
        "v",
    ])
    .unwrap();
    assert_eq!(
        cli.outer.mode,
        Mode::Remote(Remote {
            auth: Auth::Token(Token { value: "v".into() })
        })
    );
}

#[test]
fn inner_fields_are_required_only_when_both_tags_match() {
    let base = ["--ns.tls.cert", "c", "--ns.mode.type", "Remote"];
    let err = parse_outer(&base).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);

    let anonymous = [&base[..], &["--ns.mode.remote.auth.type", "Anonymous"]].concat();
    assert!(parse_outer(&anonymous).is_ok());

    let token = [&base[..], &["--ns.mode.remote.auth.type", "Token"]].concat();
    let err = parse_outer(&token).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    assert!(
        err.to_string()
            .contains("--ns.mode.remote.auth.token.value")
    );

    assert!(parse_outer(&["--ns.tls.cert", "c", "--ns.mode.type", "Off"]).is_ok());
}

#[derive(Debug, clap::Parser)]
struct PlainCli {
    #[command(flatten)]
    tls: Tls,
    #[command(flatten)]
    mode: Mode,
}

#[test]
fn eclap_args_flatten_into_a_plain_clap_parser() {
    let cli = PlainCli::try_parse_from([
        "bin",
        "--cert",
        "c",
        "--type",
        "Remote",
        "--remote.auth.type",
        "Token",
        "--remote.auth.token.value",
        "v",
    ])
    .unwrap();
    assert_eq!(cli.tls.cert, "c");
    assert!(matches!(cli.mode, Mode::Remote(_)));
}

#[test]
fn commands_have_no_id_clashes() {
    Cli::command().debug_assert();
    OuterCli::command().debug_assert();
    PlainCli::command().debug_assert();
}

#[test]
fn help_lists_docs_and_variant_headings() {
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("My service."), "{help}");
    assert!(
        help.contains("Key name registered in the coordinator."),
        "{help}"
    );
    assert!(help.contains("Must already exist."), "{help}");
    assert!(help.contains("How requests are signed."), "{help}");
    assert!(help.contains("Path of the key file."), "{help}");

    let kms = help.find("--signer.signature.type CloudKMS:").expect(&help);
    let plain = help.find("Ordinary argument").expect(&help);
    assert!(kms < help.find("--signer.signature.cloud-kms.key-ring").unwrap());
    let heading = &help[..plain]
        .lines()
        .rev()
        .find(|l| l.ends_with(':'))
        .unwrap();
    assert!(
        !heading.contains("type"),
        "ordinary arg under a variant heading:\n{help}"
    );
}

#[allow(dead_code)]
#[derive(eclap::Parser)]
struct DupCli {
    #[eclap(ns = "")]
    first: Token,
    #[eclap(ns = "")]
    second: Token,
}

#[test]
#[should_panic(expected = "duplicate flag `--value`")]
fn duplicate_flag_panics() {
    DupCli::command();
}

#[derive(Clone, Debug, PartialEq)]
struct Opaque(String);

fn parse_opaque(raw: &str) -> Result<Opaque, String> {
    Ok(Opaque(raw.to_uppercase()))
}

#[derive(Debug, eclap::Parser)]
struct Custom {
    #[arg(value_parser = parse_opaque)]
    thing: Opaque,
    #[arg(action = eclap::clap::ArgAction::Set)]
    enabled: bool,
}

#[test]
fn custom_value_parser_and_settable_bool() {
    let cli = Custom::try_parse_from(["bin", "--thing", "abc", "--enabled", "true"]).unwrap();
    assert_eq!(cli.thing, Opaque("ABC".into()));
    assert!(cli.enabled);
}

#[allow(dead_code)]
#[derive(eclap::Args)]
enum Conditional {
    Plain(WithFlatten),
}

#[allow(dead_code)]
#[derive(eclap::Args)]
struct WithFlatten {
    #[eclap(flatten)]
    metrics: Metrics,
}

#[allow(dead_code)]
#[derive(eclap::Parser)]
struct FlattenInVariant {
    #[eclap(nested)]
    mode: Conditional,
}

#[test]
#[should_panic(expected = "not supported inside enum variants (field `metrics`)")]
fn flatten_inside_variant_panics() {
    FlattenInVariant::command();
}

// Only used through `command()`: parsing it would read the ambient environment.
#[allow(dead_code)]
#[derive(eclap::Parser)]
struct EnvCli {
    #[arg(env)]
    verbose: bool,
    #[eclap(ns = "signer")]
    signer: EnvConfig,
}

#[allow(dead_code)]
#[derive(eclap::Args)]
struct EnvConfig {
    #[arg(env)]
    key: String,
    #[eclap(nested)]
    mode: Mode,
}

#[test]
fn env_names_are_derived_from_the_flag_id() {
    let cmd = EnvCli::command();
    let env = |id: &str| {
        cmd.get_arguments()
            .find(|a| a.get_id() == id)
            .unwrap()
            .get_env()
    };
    assert_eq!(env("verbose").unwrap(), "VERBOSE");
    assert_eq!(env("signer.key").unwrap(), "SIGNER__KEY");
    assert_eq!(env("signer.mode.type").unwrap(), "SIGNER__MODE__TYPE");
    assert_eq!(
        env("signer.mode.remote.auth.type").unwrap(),
        "SIGNER__MODE__REMOTE__AUTH__TYPE"
    );
    assert_eq!(env("signer.mode.remote.auth.token.value"), None);
}

#[derive(Clone, Debug, Default, PartialEq, clap::ValueEnum)]
enum Plain {
    #[value(name = "plain-a")]
    PlainA,
    #[default]
    PlainB,
}

// Parentheses are the point: requiredness must not depend on the action's syntax.
#[derive(Debug, eclap::Parser)]
struct Defaults {
    #[arg(value_enum, default_value_t = Plain::PlainA)]
    typed: Plain,
    #[arg(default_value_t, value_enum)]
    bare: Plain,
    #[arg(short = 'q', action = eclap::clap::ArgAction::Count)]
    quiet: u8,
    #[arg(action(eclap::clap::ArgAction::Count))]
    loud: u8,
    #[arg(action = (eclap::clap::ArgAction::Set))]
    value: String,
    #[arg(required = false, default_value = "x")]
    forced: String,
}

#[test]
fn value_enum_defaults_and_count_are_not_required() {
    let err = Defaults::try_parse_from(["bin"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    let cli = Defaults::try_parse_from(["bin", "--value", "v"]).unwrap();
    assert_eq!(
        (cli.typed, cli.bare, cli.quiet),
        (Plain::PlainA, Plain::PlainB, 0)
    );
    assert_eq!(
        (cli.loud, cli.forced.as_str(), cli.value.as_str()),
        (0, "x", "v")
    );
    let cli =
        Defaults::try_parse_from(["bin", "--value", "v", "-qqq", "--typed", "plain-b"]).unwrap();
    assert_eq!((cli.typed, cli.quiet), (Plain::PlainB, 3));
}

#[allow(dead_code)]
#[derive(eclap::Parser)]
struct SameLong {
    #[arg(long = "shared")]
    first: Option<String>,
    #[arg(long = "shared")]
    second: Option<String>,
}

#[test]
#[should_panic(expected = "duplicate flag `--shared`")]
fn same_long_panics() {
    SameLong::command();
}

#[allow(dead_code)]
#[derive(eclap::Parser)]
struct SameShort {
    #[arg(short = 'x')]
    first: Option<String>,
    #[arg(short = 'x')]
    second: Option<String>,
}

#[test]
#[should_panic(expected = "duplicate flag `-x`")]
fn same_short_panics() {
    SameShort::command();
}
