project(
  'girens',
  version: '3.0.0',
  meson_version: '>= 0.59.0',
)
gnome = import('gnome')

service_dir = join_paths(get_option('datadir'), 'dbus-1', 'services')
search_provider_dir = join_paths(get_option('datadir'), 'gnome-shell', 'search-providers')
pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
libexecdir = join_paths(get_option('prefix'), get_option('libexecdir'))

i18n = import('i18n')
project_id = 'nl.g4d.Girens'

conf_service = configuration_data()
conf_service.set('projectname', meson.project_name())
conf_service.set('libexecdir', libexecdir)
conf_service.set('VERSION', meson.project_version())
conf_service.set('localedir', join_paths(get_option('prefix'), get_option('localedir')))
conf_service.set('BUILD_TYPE', get_option('buildtype'))
conf_service.set('pkgdatadir', pkgdatadir)


subdir('data')
subdir('src')
subdir('po')
subdir('search-provider')

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
