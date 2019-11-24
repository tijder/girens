project(
  'girens',
  version: '1.2.0',
  meson_version: '>= 0.40.0',
)
python3 = import('python3')

service_dir = join_paths(get_option('datadir'), 'dbus-1', 'services')
search_provider_dir = join_paths(get_option('datadir'), 'gnome-shell', 'search-providers')
pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
libexecdir = join_paths(get_option('prefix'), get_option('libexecdir'))

i18n = import('i18n')
project_id = 'nl.g4d.Girens'

conf_service = configuration_data()
conf_service.set('libexecdir', libexecdir)
conf_service.set('PYTHON', python3.find_python().path())
conf_service.set('VERSION', meson.project_version())
conf_service.set('localedir', join_paths(get_option('prefix'), get_option('localedir')))
conf_service.set('pkgdatadir', pkgdatadir)
conf_service.set('PRETTYNAME', 'Girens')
conf_service.set('PACKAGE_URL', 'https://gitlab.gnome.org/tijder/girens')
conf_service.set('CONTRIBUTORS', '\n'.join([
    'Gerben Droogers'
])
)
conf_service.set('APPID', project_id)

#error(LIBEXEC_DIR)


subdir('data')
subdir('src')
subdir('po')
subdir('search-provider')

meson.add_install_script('build-aux/meson/postinstall.py')
