pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
moduledir = join_paths(pkgdatadir, 'girens')
gnome = import('gnome')

gnome.compile_resources('girens',
  'girens.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
)

python3 = import('python3')

conf = configuration_data()
conf.set('PYTHON', python3.find_python().path())
conf.set('VERSION', meson.project_version())
conf.set('localedir', join_paths(get_option('prefix'), get_option('localedir')))
conf.set('pkgdatadir', pkgdatadir)

configure_file(
  input: 'girens.in',
  output: 'girens',
  configuration: conf,
  install: true,
  install_dir: get_option('bindir')
)

girens_sources = [
  '__init__.py',
  'plex.py',
  'player.py',
  'gi_composites.py',
  'main.py',
  'profile_dialog.py',
  'window.py',
  'sidebar_box.py',
  'media_box.py',
  'login_view.py',
  'discover_view.py',
  'show_view.py',
  'section_view.py',
  'search_view.py',
  'cover_box.py',
  'section_grid.py',
  'loading_view.py',
  'download_row.py',
  'download_menu.py'
]

install_data(girens_sources, install_dir: moduledir)