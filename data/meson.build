desktop_file = i18n.merge_file(
  input: 'nl.g4d.Girens.desktop.in',
  output: 'nl.g4d.Girens.desktop',
  type: 'desktop',
  po_dir: '../po',
  install: true,
  install_dir: join_paths(get_option('datadir'), 'applications')
)

desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('Validate desktop file', desktop_utils,
    args: [desktop_file]
  )
endif

appstream_file = i18n.merge_file(
  input: 'nl.g4d.Girens.appdata.xml.in',
  output: 'nl.g4d.Girens.appdata.xml',
  po_dir: '../po',
  install: true,
  install_dir: join_paths(get_option('datadir'), 'appdata')
)

appstream_util = find_program('appstream-util', required: false)
if appstream_util.found()
  validate_args = ['validate', appstream_file]
	if not get_option('network_tests')
	  validate_args += '--nonet'
	endif
  test('Validate appstream file', appstream_util, args: validate_args)
endif

install_data('nl.g4d.Girens.gschema.xml',
  install_dir: join_paths(get_option('datadir'), 'glib-2.0/schemas')
)

compile_schemas = find_program('glib-compile-schemas', required: false)
if compile_schemas.found()
  test('Validate schema file', compile_schemas,
    args: ['--strict', '--dry-run', meson.current_source_dir()]
  )
endif

subdir('ui')
subdir('icons')
