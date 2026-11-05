# The girens-sp binary itself is built by Cargo, see src/meson.build.
configure_file(
  input: project_id + '.SearchProvider.service.in',
  output: project_id + '.SearchProvider.service',
  configuration: conf_service,
  install_dir: service_dir
)
install_data(
  'nl.g4d.Girens.SearchProvider.ini',
  install_dir: search_provider_dir
)
