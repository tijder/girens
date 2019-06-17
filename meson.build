project('girens',         version: '0.1.0',
  meson_version: '>= 0.40.0',
)

i18n = import('i18n')
project_id = 'nl.g4d.Girens'

#error(LIBEXEC_DIR)


subdir('data')
subdir('src')
subdir('po')
subdir('search-provider')

meson.add_install_script('build-aux/meson/postinstall.py')
