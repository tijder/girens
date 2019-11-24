pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
moduledir = join_paths(pkgdatadir, 'girens')

configure_file(
  input: 'girens.in',
  output: 'girens',
  configuration: conf_service,
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
  'media_box_music.py',
  'media_box_video_bottom.py',
  'media_box_video_top.py',
  'login_view.py',
  'discover_view.py',
  'show_view.py',
  'section_view.py',
  'search_view.py',
  'player_view.py',
  'cover_box.py',
  'section_grid.py',
  'loading_view.py',
  'sync_dialog.py',
  'sync_item.py',
  'sync_settings.py',
  'download_row.py',
  'download_menu.py',
  'playqueue_popover.py',
  'playqueue_item.py',
  'artist_view.py',
  'album_view.py',
  'album_item.py',
  'resume_dialog.py',
  'remote_player.py',
  'mpris.py'
]

install_data(girens_sources, install_dir: moduledir)
