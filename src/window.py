# window.py
#
# Copyright 2018 Gerben Droogers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gettext import gettext as _
from gi.repository import Gtk, Gdk, Gio, GLib, GdkPixbuf, Adw


from .sidebar_box import SidebarBox
from .media_box import MediaBox
from .media_box_music import MediaBoxMusic
from .login_view import LoginView
from .discover_view import DiscoverView
from .show_view import ShowView
from .section_view import SectionView
from .search_view import SearchView
from .profile_dialog import ProfileDialog
from .loading_view import LoadingView
from .sync_dialog import SyncDialog
from .artist_view import ArtistView
from .player_view import PlayerView
from .album_view import AlbumView
from .download_menu import DownloadMenu
from .mpris import MediaPlayer2Service
from .remote_player import RemotePlayer
from .sync_settings import SyncSettings
from plex_remote.plex_remote_client import PlexRemoteClient
from .theme_switcher import ThemeSwitcher

from .plex import Plex
from .player import Player

from .constants import build_type

import os
import threading
import time

@Gtk.Template(resource_path='/nl/g4d/Girens/main_window.ui')
class PlexWindow(Adw.ApplicationWindow):
    __gtype_name__ = 'PlexWindow'

    _active_view = None
    _show_id = None
    _remote_client_active = None

    _style_manager = Adw.StyleManager.get_default()

    _content_box_wrapper = Gtk.Template.Child()
    _content_leaflet = Gtk.Template.Child()

    content_header = Gtk.Template.Child()
    separator_header = Gtk.Template.Child()
    sidebar_leaflet = Gtk.Template.Child()

    _viewStack_frame = Gtk.Template.Child()

    _viewStack_pages = Gtk.Template.Child()
    _login_revealer = Gtk.Template.Child()
    _discover_revealer = Gtk.Template.Child()
    _show_revealer = Gtk.Template.Child()
    _section_revealer = Gtk.Template.Child()
    _search_revealer = Gtk.Template.Child()
    _artist_revealer = Gtk.Template.Child()
    _album_revealer = Gtk.Template.Child()
    _player_revealer = Gtk.Template.Child()

    _discover_view = Gtk.Template.Child()
    _show_view = Gtk.Template.Child()
    _section_view = Gtk.Template.Child()
    _search_view = Gtk.Template.Child()
    _artist_view = Gtk.Template.Child()
    _album_view = Gtk.Template.Child()
    _player_view = Gtk.Template.Child()
    _login_view = Gtk.Template.Child()
    _loading_view = Gtk.Template.Child()

    _media_box_music = Gtk.Template.Child()

    header = Gtk.Template.Child()
    sidebar = Gtk.Template.Child()
    _sidebar_viewport = Gtk.Template.Child()

    _search_bar = Gtk.Template.Child()
    _search_entry = Gtk.Template.Child()

    _avatar_image = Gtk.Template.Child()
    _profile_button = Gtk.Template.Child()
    _menu_button = Gtk.Template.Child()
    _sync_button = Gtk.Template.Child()
    _shortcuts_button = Gtk.Template.Child()
    _sync_image = Gtk.Template.Child()
    _download_button = Gtk.Template.Child()
    _back_button = Gtk.Template.Child()
    _search_toggle_button = Gtk.Template.Child()
    _prefer_music_clips_check_button = Gtk.Template.Child()
    _advertise_as_client_check_button = Gtk.Template.Child()
    _about_button = Gtk.Template.Child()
    _volume_adjustment = Gtk.Template.Child()

    _menu_popover = Gtk.Template.Child()

    _transcode_media_switch = Gtk.Template.Child()
    _res_set_1080 = Gtk.Template.Child()
    _res_set_720 = Gtk.Template.Child()
    _res_set_480 = Gtk.Template.Child()
    _res_set_240 = Gtk.Template.Child()

    _window_placement_update_timeout = None

    def __init__(self, show_id=None, video_output_driver=None, deinterlace=None, **kwargs):
        super().__init__(**kwargs)

        if build_type == "debug":
            self.get_style_context().add_class("devel")

        self.__custom_css()

        self._video_output_driver = "xv,"
        self._deinterlace = "no"
        if video_output_driver != None:
            self._video_output_driver = video_output_driver
        if deinterlace != None:
            self._deinterlace = deinterlace

        self._aplication = kwargs["application"]
        self._show_id = show_id

        self.connect("map", self.__screen_mapped)
        self.connect("unrealize", self.__on_destroy)

        action = Gio.SimpleAction(name="show-album-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_show_album_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="show-artist-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_show_artist_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="show-show-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_show_show_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="play-item", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_play_item)
        self.add_action(action)
        action = Gio.SimpleAction(name="play-item-from-beginning", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_play_item_from_beginning)
        self.add_action(action)

        action = Gio.SimpleAction(name="mark-as-played-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_mark_as_played_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="mark-as-unplayed-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_mark_as_unplayed_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="sync-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_sync_by_id)
        self.add_action(action)
        action = Gio.SimpleAction(name="shuffle-by-id", parameter_type=GLib.VariantType.new('x'))
        action.connect("activate", self.on_shuffle_by_id)
        self.add_action(action)

    def on_show_album_by_id(self, action, parameter):
        self.__on_go_to_album(parameter.get_int64())

    def on_show_artist_by_id(self, action, parameter):
        self.__on_go_to_artist(parameter.get_int64())

    def on_show_show_by_id(self, action, parameter):
        self.__on_go_to_show(parameter.get_int64())

    def on_play_item(self, action, parameter):
        thread = threading.Thread(target=self._plex.play_item, args=(parameter.get_int64(),))
        thread.daemon = True
        thread.start()

    def on_play_item_from_beginning(self, action, parameter):
        thread = threading.Thread(target=self._plex.play_item, args=(parameter.get_int64(),),kwargs={'from_beginning':True})
        thread.daemon = True
        thread.start()

    def on_mark_as_played_by_id(self, action, parameter):
        thread = threading.Thread(target=self._plex.mark_as_played, args=(parameter.get_int64(),))
        thread.daemon = True
        thread.start()

    def on_mark_as_unplayed_by_id(self, action, parameter):
        thread = threading.Thread(target=self._plex.mark_as_unplayed, args=(parameter.get_int64(),))
        thread.daemon = True
        thread.start()

    def on_sync_by_id(self, action, parameter):
        item = self._plex.get_item(parameter.get_int64())
        if (item.TYPE == 'show'):
            self._sync_settings = SyncSettings(self._plex, item)
            self._sync_settings.set_transient_for(self)
            self._sync_settings.show()
        else:
            thread = threading.Thread(target=self._plex.add_to_sync, args=(item,))
            thread.daemon = True
            thread.start()

    def on_shuffle_by_id(self, action, parameter):
        thread = threading.Thread(target=self._plex.play_item, args=(parameter.get_int64(),),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()


    def __on_destroy(self, widget):
        if self._remote_client_active is True:
            thread = threading.Thread(target=self.plexRemoteClient.stop)
            thread.daemon = True
            thread.start()

    def show_by_id(self, show_id):
        item = self._plex.get_item(show_id[1])
        if (item.TYPE == 'artist'):
            self.__on_go_to_artist(show_id[1])
        elif (item.TYPE == 'album'):
            self.__on_go_to_album(show_id[1])
        elif (item.TYPE == 'show'):
            self.__on_go_to_show(show_id[1])

    def set_video_output_driver(self, video_output_driver):
        self._video_output_driver = video_output_driver
        self._player.set_video_output_driver(self._video_output_driver)

    def set_deinterlace(self, deinterlace):
        self._deinterlace = deinterlace
        self._player.set_deinterlace(self._deinterlace)

    def __screen_mapped(self, map):
        self._settings = Gio.Settings ("nl.g4d.Girens")

        #self._player_view = PlayerView(self)
        self._player_view.connect("fullscreen", self.__fullscreen)
        self._player_view.connect("windowed", self.__windowed)
        #self._player_revealer.set_child(self._player_view)

        self._player = Player(self._player_view)
        self._player.set_video_output_driver(self._video_output_driver)
        self._player.set_deinterlace(self._deinterlace)
        self._player.connect("video-starting", self.__on_video_starting)
        self._player_view.set_player(self._player)
        self._plex = Plex(os.environ['XDG_CONFIG_HOME'], os.environ['XDG_CACHE_HOME'], self._player)
        self._player_view.set_plex(self._plex)
        self._player_view.set_window(self)
        self._plex.connect("download-from-url", self.__on_downloaded)
        self._plex.connect("sync-status", self.__on_sync)
        self._plex.connect("connection-to-server", self.__on_connection_to_server)
        self._plex.connect("logout", self.__on_logout)
        self._plex.connect("loading", self.__on_plex_load)
        self._player.connect("play-music-clip-instead-of-track", self.__on_prefer_music_clips_changed)
        self._player_view._frame.realize()

        self._back_button.connect("clicked", self.__on_back_clicked)
        self._profile_button.connect("clicked", self.__on_profile_clicked)
        self._about_button.connect("activated", self.__on_about_clicked)

        self._download_menu = DownloadMenu(self._plex)
        self._download_menu.connect("show-button", self.__on_show_download_button)
        self._download_button.set_popover(self._download_menu)
        self._download_button.set_visible(False)

        self._sync_button.connect("clicked", self.__on_sync_clicked)
        self._shortcuts_button.connect("activated", self.__on_shortcuts_activate)

        self._loading_view.set_plex(self._plex)

        self._media_box = MediaBox(self._plex, self._player, show_only_type="audio")
        self._media_box.set_music_ui(self._media_box_music)

        self._search_toggle_button.connect("toggled", self.__on_search_toggled)
        self._search_entry.connect("search-changed", self.__on_search_changed)
        self._search_entry.connect("stop-search", self.__stop_search)

        self._sidebar_box = SidebarBox(self._plex, self._player)
        self._sidebar_box.connect("section-clicked", self.__on_section_clicked)
        self._sidebar_box.connect("home-button-clicked", self.__on_home_clicked)
        self._sidebar_box.connect("playlists-button-clicked", self.__on_playlists_clicked)
        self._sidebar_box.connect("player-button-clicked", self.__on_player_clicked)
        self._sidebar_box.connect("other-server-selected", self.__on_other_server_selected)
        self._sidebar_viewport.set_child(self._sidebar_box)

        self._section_view.set_plex(self._plex)

        self._search_view.set_plex(self._plex)

        self._discover_view.set_plex(self._plex)

        self._show_view.set_plex(self._plex)

        self._artist_view.set_plex(self._plex)

        self._album_view.set_plex(self._plex)

        self._login_view.set_plex(self._plex)
        self._login_view.connect("login-success", self.__on_login_success)
        self._login_view.connect("login-failed", self.__on_login_failed)
        self._login_view.connect("login-not-found", self.__on_login_not_found)

        remote_player = RemotePlayer(self._player, self)

        self.plexRemoteClient = PlexRemoteClient(remote_player)

        self._prefer_music_clips_check_button.connect("state-set", self.__on_prefer_music_clips_check_button_clicked)
        self._settings.bind ("prefer-music-clips", self._prefer_music_clips_check_button, "active", Gio.SettingsBindFlags.DEFAULT);
        #self._settings.bind ("play-media-direct", self._direct_play_check_button, "active", Gio.SettingsBindFlags.DEFAULT);
        self._advertise_as_client_check_button.connect("state-set", self.__advertise_as_client_check_button_clicked)
        self._settings.bind ("advertise-as-client", self._advertise_as_client_check_button, "active", Gio.SettingsBindFlags.DEFAULT);
        self._settings.bind ("volume-level", self._volume_adjustment, "value", Gio.SettingsBindFlags.DEFAULT);

        sr = self._settings.get_string("transcode-media-to-resolution")
        if (sr == "1920x1080"):
            self._res_set_1080.set_active(True)
        elif (sr == "1280x720"):
            self._res_set_720.set_active(True)
        elif (sr == "854x480"):
            self._res_set_480.set_active(True)
        elif (sr == "427x240"):
            self._res_set_240.set_active(True)

        self._res_set_1080.connect("toggled", self.__res_changed_1080)
        self._res_set_720.connect("toggled", self.__res_changed_720)
        self._res_set_480.connect("toggled", self.__res_changed_480)
        self._res_set_240.connect("toggled", self.__res_changed_240)
        self._settings.bind ("play-media-direct", self._transcode_media_switch, "enable-expansion", Gio.SettingsBindFlags.INVERT_BOOLEAN);

        width = self._settings.get_int("window-size-width")
        height = self._settings.get_int("window-size-height")
        self.set_default_size(width, height)

        MediaPlayer2Service(self)

        #self.connect("configure-event", self.__on_configure_event)
        #self.connect("window-state-event", self.__on_window_state_event)
        self.connect("notify::default-width", self.__on_width_changed)
        self.connect("notify::default-height", self.__on_height_changed)
        self.connect("notify::fullscreened", self.fullscreened)

        self.__show_loading_view(True, _('Starting Girens'))
        self._login_view.try_login()

    def __res_changed_1080(self, res_button):
        if(res_button.get_active()):
            self._settings.set_string("transcode-media-to-resolution", "1920x1080")

    def __res_changed_720(self, res_button):
        if(res_button.get_active()):
            self._settings.set_string("transcode-media-to-resolution", "1280x720")

    def __res_changed_480(self, res_button):
        if(res_button.get_active()):
            self._settings.set_string("transcode-media-to-resolution", "854x480")

    def __res_changed_240(self, res_button):
        if(res_button.get_active()):
            self._settings.set_string("transcode-media-to-resolution", "427x240")

    def __show_login_view(self):
        self._viewStack_frame.set_visible_child(self._login_view)

    def __on_login_not_found(self, view):
        self.__show_loading_view(False, '')
        self.__show_login_view()

    def __on_login_failed(self,view):
        self.__show_loading_view(False, '')
        self.__show_login_view()


    def __on_login_success(self, view, status):
        if (status == True):
            self._viewStack_frame.set_visible_child(self.sidebar_leaflet)
            thread = threading.Thread(target=self._plex.connect_to_server)
            thread.daemon = True
            thread.start()
        else:
            self.destroy()

    def __on_other_server_selected(self, sidebox, resource):
        thread = threading.Thread(target=self._plex.connect_to_resource, args=(resource,))
        thread.daemon = True
        thread.start()

    def __on_logout(self, plex):
        self.__show_login_view()

    def __on_connection_to_server(self, plex):
        self._sidebar_box.refresh()
        self._sync_dialog = SyncDialog(self._plex)
        self._sync_dialog.set_transient_for(self)
        if (self._show_id is not None):
            self.show_by_id(self._show_id)
        thread = threading.Thread(target=self._plex.reload_search_provider_data)
        thread.daemon = True
        thread.start()
        if (self._plex.has_token()):
            thread = threading.Thread(target=self._plex.download_from_url, args=(self._plex._account.username, self._plex._account.thumb))
            thread.daemon = True
            thread.start()

    def __on_downloaded(self, plex, name_image, path):
        if(self._plex._account.username == name_image):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 25, 25)
            GLib.idle_add(self.__set_image, pix)

    def __on_sync_clicked(self, button):
        self._sync_dialog.show()

    def __on_sync(self, plex, status):
        GLib.idle_add(self.__set_sync, status)

    def __set_sync(self, status):
        if (status == True):
            self._sync_button.set_sensitive(False)
            self._sync_image.set_from_icon_name('network-transmit-receive-symbolic')
        else:
            self._sync_button.set_sensitive(True)
            self._sync_image.set_from_icon_name('network-transmit-symbolic')

    def __set_image(self, pix):
        self._avatar_image.set_from_pixbuf(pix)

    def __on_refresh_clicked(self, button):
        self.__refresh_data()

    def __on_home_clicked(self, view):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._viewStack_pages.set_visible_child(self._discover_view)
        self._discover_view.refresh()

    def __on_playlists_clicked(self, view):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._section_view.show_playlists()
        self._viewStack_pages.set_visible_child(self._section_view)

    def __on_player_clicked(self, view):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._viewStack_pages.set_visible_child(self._player_view)

    def __on_section_clicked(self, view, section):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._section_view.refresh(section)
        #self.__show_view('section')
        self._viewStack_pages.set_visible_child(self._section_view)

    def __on_back_clicked(self, button):
        self._sidebar_box.unselect_all()
        self.sidebar_leaflet.set_visible_child(self.header)

    def __refresh_data(self):
        if(self._active_view == 'discover'):
            self._discover_view.refresh()
        
    def __on_go_to_show_clicked(self, view, key):
        self.__on_go_to_show(key)

    def __on_go_to_show(self, key):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._show_view.change_show(key)
        self._viewStack_pages.set_visible_child(self._show_view)
        self._sidebar_box.unselect_all()

    def __on_go_to_artist_clicked(self, view, key):
        self.__on_go_to_artist(key)

    def __on_go_to_artist(self, key):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._artist_view.change_artist(key)
        self._viewStack_pages.set_visible_child(self._artist_view)
        self._sidebar_box.unselect_all()

    def __on_go_to_album_clicked(self, view, key):
        self.__on_go_to_album(key)

    def __on_go_to_album(self, key):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._album_view.change_album(key)
        self._viewStack_pages.set_visible_child(self._album_view)
        self._sidebar_box.unselect_all()

    def __on_video_starting(self, widget):
        GLib.idle_add(self.__go_to_player)

    def __go_to_player(self):
        self.sidebar_leaflet.set_visible_child(self._content_leaflet)
        self._viewStack_pages.set_visible_child(self._player_view)
        self._player.view_shown()

    def __stop_search(self, search):
        self._search_toggle_button.set_active(False)

    def __on_search_toggled(self, toggle):
        self._search_bar.set_search_mode(toggle.get_active())

    def __on_search_changed(self, entry):
        if (entry.get_text() != "" and len(entry.get_text()) >= 3):
            self._search_view.refresh(entry.get_text())
            self._viewStack_pages.set_visible_child(self._search_view)

    def __on_profile_clicked(self, button):
        self._profile_dialog = ProfileDialog(self._plex)
        self._profile_dialog.set_transient_for(self)
        self._profile_dialog.show()

    def __on_prefer_music_clips_check_button_clicked(self, button, state):
        self._player.set_play_music_clip_instead_of_track(state)

    def __on_prefer_music_clips_changed(self, player, value):
        self._prefer_music_clips_check_button.set_active(value)

    def __advertise_as_client_check_button_clicked(self, button, state):
        if state:
            thread = threading.Thread(target=self.plexRemoteClient.start)
            thread.daemon = True
            thread.start()
            self._remote_client_active = True
        else:
            thread = threading.Thread(target=self.plexRemoteClient.stop)
            thread.daemon = True
            thread.start()
            self._remote_client_active = False

    def __on_about_clicked(self, button):
        builder = Gtk.Builder()
        builder.add_from_resource("/nl/g4d/Girens/about_dialog.ui")
        about_dialog = builder.get_object("about_dialog")
        about_dialog.set_modal(True)
        if self is not NotImplemented:
            about_dialog.set_transient_for(self)
        about_dialog.present()
        self._menu_popover.popdown()

    def __on_plex_load(self, plex, load_text, status):
        self.__show_loading_view(status, load_text)

    def __show_loading_view(self, show, load_text):
        self._loading_view.set_text(load_text)
        if (show):
            self._viewStack_frame.set_visible_child(self._loading_view)
        else:
            self._viewStack_frame.set_visible_child(self.sidebar_leaflet)

    def go_fullscreen(self):
        self._player_view.go_fullscreen()

    def __fullscreen(self, widged, booleon):
        if booleon:
            self.fullscreen()
        else:
            self.unfullscreen()

    def __windowed(self, widged, booleon):
        if booleon:
            self.__add_extra_widgets()
        else:
            self.__remove_extra_widgets()

    def __remove_extra_widgets(self):
        self.content_header.set_visible(False)
        self.separator_header.set_visible(False)
        self.header.set_visible(False)

        #self._media_box.set_visible(False)
        #self.sidebar.hide()
        #self.separator.hide()
        #self.get_style_context().add_class("black_background")
        #self._main_scrolled_window.get_style_context().add_class("black_background")

    def __add_extra_widgets(self):
        self.content_header.set_visible(True)
        self.separator_header.set_visible(True)
        self.header.set_visible(True)

        #self._media_box.set_visible(True)
        #self.sidebar.show()
        #self.separator.show()
        #self.get_style_context().remove_class("black_background")
        #self._main_scrolled_window.get_style_context().remove_class("black_background")

    def fullscreened(self, widget, state):
        if (widget.is_fullscreen()): # Is fullscreen
            self._player_view.set_fullscreen_state()
            self.__remove_extra_widgets()
        else: # Is not fullscreen
            self._player_view.set_unfullscreen_state()
            self.__add_extra_widgets()



    def __on_show_download_button(self, menu):
        self._download_button.set_visible(True)

    def __on_shortcuts_activate(self, button):
        builder = Gtk.Builder()
        builder.add_from_resource("/nl/g4d/Girens/shortcuts.ui")
        builder.get_object("shortcuts").set_transient_for(self)
        builder.get_object("shortcuts").show()
        self._menu_popover.popdown()


    def __on_configure_event(self, widget, event):
        if self._window_placement_update_timeout is None:
            self._window_placement_update_timeout = GLib.timeout_add(
                500, self.__update_screen_size_change, widget)

    def __on_height_changed(self, widget, event):
        if self._window_placement_update_timeout is None:
            self._window_placement_update_timeout = GLib.timeout_add(
                500, self.__update_screen_size_change, widget)

    def __on_width_changed(self, widget, event):
        if self._window_placement_update_timeout is None:
            self._window_placement_update_timeout = GLib.timeout_add(
                500, self.__update_screen_size_change, widget)

    def __update_screen_size_change(self, widget):
        size = widget.get_default_size()
        if not self._player_view._fullscreen:
            self._settings.set_int("window-size-width", size[0])
            self._settings.set_int("window-size-height", size[1])
        self._media_box_music.width_changed(size[0])
        self._section_view.width_changed(size[0])
        self._player_view.width_changed(size[0])
        self._discover_view.width_changed(size[0])
        self._album_view.width_changed(size[0])
        self._artist_view.width_changed(size[0])
        self._show_view.width_changed(size[0])
        GLib.source_remove(self._window_placement_update_timeout)
        self._window_placement_update_timeout = None
        return False

    def __custom_css(self):
        css_provider = Gtk.CssProvider()
        css_provider_resource = Gio.File.new_for_uri(
            "resource:///nl/g4d/Girens/plex.css")
        css_provider.load_from_file(css_provider_resource)

        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), css_provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        
