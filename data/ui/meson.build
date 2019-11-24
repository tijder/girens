gnome.compile_resources('girens',
  '../nl.g4d.Girens.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
  dependencies: configure_file(
    input: 'about_dialog.ui.in',
    output: 'about_dialog.ui',
    configuration: conf_service
  )
)
