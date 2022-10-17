pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
moduledir = join_paths(pkgdatadir, 'girens')

configure_file(
  input: 'girens.in',
  output: 'girens',
  configuration: conf_service,
  install: true,
  install_dir: get_option('bindir')
)

configure_file(
  input: 'constants.py.in',
  output: 'constants.py',
  configuration: conf_service,
  install: true,
  install_dir: moduledir,
)

girens_sources = [
  '__init__.py',
  'album_item.py',
  'album_view.py',
  'artist_view.py',
  'cover_box.py',
  'discover_view.py',
  'download_menu.py',
  'download_row.py',
  'item_bin.py',
  'list.py',
  'list_playqueue.py',
  'loading_view.py',
  'login_view.py',
  'main.py',
  'media_box.py',
  'media_box_music.py',
  'media_box_video_bottom.py',
  'media_box_video_top.py',
  'mpris.py',
  'music_popover_menu.py',
  'player.py',
  'player_view.py',
  'playqueue_item.py',
  'playqueue_popover.py',
  'plex.py',
  'profile_dialog.py',
  'remote_player.py',
  'resume_dialog.py',
  'search_view.py',
  'section_grid.py',
  'section_view.py',
  'show_view.py',
  'sidebar_box.py',
  'sync_dialog.py',
  'sync_item.py',
  'sync_settings.py',
  'window.py',
]

install_data(girens_sources, install_dir: moduledir)
