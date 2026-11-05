cargo = find_program('cargo')

cargo_options = [
  '--manifest-path', meson.project_source_root() / 'Cargo.toml',
  '--target-dir', meson.project_build_root() / 'target',
]

if get_option('buildtype') == 'debug'
  rust_target = 'debug'
else
  cargo_options += ['--release']
  rust_target = 'release'
endif

# The Flatpak build points CARGO_HOME at the vendored crates (see
# cargo-sources.json); otherwise Cargo keeps its files in the build directory.
cargo_home = run_command('printenv', 'CARGO_HOME', check: false).stdout().strip()
if cargo_home == ''
  cargo_home = meson.project_build_root() / 'cargo-home'
endif

# The GIRENS_* values are read by src/config.rs through option_env!().
cargo_env = [
  'CARGO_HOME=' + cargo_home,
  'GIRENS_VERSION=' + meson.project_version(),
  'GIRENS_PKGDATADIR=' + pkgdatadir,
  'GIRENS_LOCALEDIR=' + join_paths(get_option('prefix'), get_option('localedir')),
  'GIRENS_BUILD_TYPE=' + get_option('buildtype'),
]

# Builds the application and the GNOME Shell search provider.
custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: [meson.project_name(), 'girens-sp'],
  console: true,
  install: true,
  install_dir: [get_option('bindir'), libexecdir],
  command: [
    'env', cargo_env,
    cargo, 'build', cargo_options, '-p', 'girens', '-p', 'girens-sp',
    '&&',
    'cp',
    meson.project_build_root() / 'target' / rust_target / meson.project_name(),
    meson.project_build_root() / 'target' / rust_target / 'girens-sp',
    '@OUTDIR@',
  ],
)
