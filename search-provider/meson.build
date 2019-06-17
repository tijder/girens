service_dir = join_paths(get_option('datadir'), 'dbus-1', 'services')
search_provider_dir = join_paths(get_option('datadir'), 'gnome-shell', 'search-providers')


pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
libexecdir = join_paths(get_option('prefix'), get_option('libexecdir'))

conf_service = configuration_data()
conf_service.set('libexecdir', libexecdir)
conf_service.set('PYTHON', python3.find_python().path())
conf_service.set('VERSION', meson.project_version())
conf_service.set('localedir', join_paths(get_option('prefix'), get_option('localedir')))
conf_service.set('pkgdatadir', pkgdatadir)

configure_file(
  input: 'girens-sp.in',
  output: 'girens-sp',
  configuration: conf_service,
  install: true,
  install_dir: libexecdir
)
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
