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

from gi.repository import Gtk, Gdk, Gio, GLib, GdkPixbuf
from .gi_composites import GtkTemplate

from .sidebar_box import SidebarBox
from .media_box import MediaBox
from .login_view import LoginView
from .discover_view import DiscoverView
from .show_view import ShowView
from .section_view import SectionView
from .search_view import SearchView
from .profile_dialog import ProfileDialog
from .loading_view import LoadingView
from .sync_dialog import SyncDialog
from .artist_view import ArtistView
from .album_view import AlbumView
from .download_menu import DownloadMenu
from .resume_dialog import ResumeDialog

from .plex import Plex
from .player import Player

import os
import threading

@GtkTemplate(ui='/nl/g4d/Girens/main_window.ui')
class PlexWindow(Gtk.ApplicationWindow):
    __gtype_name__ = 'PlexWindow'

    _active_view = None

    _content_box_wrapper = GtkTemplate.Child()
    _content_leaflet = GtkTemplate.Child()

    _discover_revealer = GtkTemplate.Child()
    _show_revealer = GtkTemplate.Child()
    _section_revealer = GtkTemplate.Child()
    _search_revealer = GtkTemplate.Child()
    _artist_revealer = GtkTemplate.Child()
    _album_revealer = GtkTemplate.Child()

    header = GtkTemplate.Child()
    sidebar = GtkTemplate.Child()
    _sidebar_viewport = GtkTemplate.Child()

    _search_bar = GtkTemplate.Child()
    _search_entry = GtkTemplate.Child()

    _avatar_image = GtkTemplate.Child()
    _profile_button = GtkTemplate.Child()
    _sync_button = GtkTemplate.Child()
    _sync_image = GtkTemplate.Child()
    _download_button = GtkTemplate.Child()
    _back_button = GtkTemplate.Child()
    _search_toggle_button = GtkTemplate.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self.__custom_css()

        self.connect("map", self.__screen_mapped)


    def __screen_mapped(self, map):
        resume_dialog = ResumeDialog()
        resume_dialog.set_transient_for(self)
        self._player = Player(resume_dialog)
        self._plex = Plex(os.environ['XDG_CONFIG_HOME'], os.environ['XDG_CACHE_HOME'], self._player)
        self._plex.connect("download-from-url", self.__on_downloaded)
        self._plex.connect("sync-status", self.__on_sync)
        self._plex.connect("connection-to-server", self.__on_connection_to_server)
        self._plex.connect("logout", self.__on_logout)
        self._plex.connect("loading", self.__on_plex_load)

        self._back_button.connect("clicked", self.__on_back_clicked)
        self._profile_button.connect("clicked", self.__on_profile_clicked)

        self._download_menu = DownloadMenu(self._plex)
        self._download_menu.connect("show-button", self.__on_show_download_button)
        self._download_button.set_popover(self._download_menu)
        self._download_button.set_visible(False)

        self._sync_button.connect("clicked", self.__on_sync_clicked)

        self._loading_view = LoadingView(self._plex)
        self._content_box_wrapper.add(self._loading_view)
        self._loading_view.set_visible(False)
        self._loading_view.set_vexpand(True)

        self._media_box = MediaBox(self._plex, self._player)
        self._content_box_wrapper.add(self._media_box)
        self._media_box.set_visible(True)

        self._search_toggle_button.connect("toggled", self.__on_search_toggled)
        self._search_entry.connect("search-changed", self.__on_search_changed)
        self._search_entry.connect("stop-search", self.__stop_search)

        self._sidebar_box = SidebarBox(self._plex)
        self._sidebar_box.connect("section-clicked", self.__on_section_clicked)
        self._sidebar_box.connect("home-button-clicked", self.__on_home_clicked)
        self._sidebar_box.connect("playlists-button-clicked", self.__on_playlists_clicked)
        self._sidebar_viewport.add(self._sidebar_box)

        self._section_view = SectionView(self._plex)
        self._section_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._section_view.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        self._section_revealer.add(self._section_view)

        self._search_view = SearchView(self._plex)
        self._search_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._search_revealer.add(self._search_view)

        self._discover_view = DiscoverView(self._plex)
        self._discover_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._discover_view.connect("view-album-wanted", self.__on_go_to_album_clicked)
        self._discover_view.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        self._discover_revealer.add(self._discover_view)

        self._ShowView = ShowView(self._plex)
        self._show_revealer.add(self._ShowView)

        self._artist_view = ArtistView(self._plex)
        self._artist_revealer.add(self._artist_view)

        self._album_view = AlbumView(self._plex)
        self._album_view.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        self._album_revealer.add(self._album_view)

        self.__show_login_view()

    def __show_view(self, view_name):
        self._discover_revealer.set_visible(False)
        self._show_revealer.set_visible(False)
        self._section_revealer.set_visible(False)
        self._search_revealer.set_visible(False)
        self._artist_revealer.set_visible(False)
        self._album_revealer.set_visible(False)

        if view_name == 'discover':
            self._discover_revealer.set_visible(True)
        elif view_name == 'show':
            self._show_revealer.set_visible(True)
        elif view_name == 'section':
            self._section_revealer.set_visible(True)
        elif view_name == 'search':
            self._search_revealer.set_visible(True)
        elif view_name == 'artist':
            self._artist_revealer.set_visible(True)
        elif view_name == 'album':
            self._album_revealer.set_visible(True)

        if (view_name != 'search'):
            self._search_toggle_button.set_active(False)

        self._active_view = view_name

    def __show_login_view(self):
        self._content_box_wrapper.set_visible(False)
        self._login_view = LoginView(self._plex)
        self._login_view.connect("login-success", self.__on_login_success)
        self._login_view.set_transient_for(self)

    def __on_login_success(self, view, status):
        if (status == True):
            self._content_box_wrapper.set_visible(True)
            self.__show_view('discover')
            thread = threading.Thread(target=self._plex.connect_to_server)
            thread.daemon = True
            thread.start()
        else:
            self.destroy()

    def __on_logout(self, plex):
        self.__show_login_view()

    def __on_connection_to_server(self, plex):
        self._discover_view.refresh()
        self._sidebar_box.refresh()
        self._sync_dialog = SyncDialog(self._plex)
        self._sync_dialog.set_transient_for(self)
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
            self._sync_image.set_from_icon_name('network-transmit-receive-symbolic', 4)
        else:
            self._sync_button.set_sensitive(True)
            self._sync_image.set_from_icon_name('network-transmit-symbolic', 4)

    def __set_image(self, pix):
        self._avatar_image.set_from_pixbuf(pix)

    def __on_refresh_clicked(self, button):
        self.__refresh_data()

    def __on_home_clicked(self, view):
        self.header.set_visible_child_name("content");
        self._discover_view.refresh()
        self.__show_view('discover')

    def __on_playlists_clicked(self, view):
        self.header.set_visible_child_name("content");
        self._section_view.show_playlists()
        self.__show_view('section')

    def __on_section_clicked(self, view, section):
        self.header.set_visible_child_name("content");
        self._section_view.refresh(section)
        self.__show_view('section')

    def __on_back_clicked(self, button):
        self._sidebar_box.unselect_all()
        self.header.set_visible_child_name("sidebar");

    def __refresh_data(self):
        if(self._active_view == 'discover'):
            self._discover_view.refresh()
        
    def __on_go_to_show_clicked(self, view, key):
        self._ShowView.change_show(key)
        self.__show_view('show')

    def __on_go_to_artist_clicked(self, view, key):
        self._artist_view.change_artist(key)
        self.__show_view('artist')

    def __on_go_to_album_clicked(self, view, key):
        self._album_view.change_album(key)
        self.__show_view('album')

    def __stop_search(self, search):
        self._search_toggle_button.set_active(False)

    def __on_search_toggled(self, toggle):
        self._search_bar.set_search_mode(toggle.get_active())

    def __on_search_changed(self, entry):
        if (entry.get_text() != "" and len(entry.get_text()) >= 3):
            self.header.set_visible_child_name("content");
            self._search_view.refresh(entry.get_text())
            self.__show_view('search')

    def __on_profile_clicked(self, button):
        self._profile_dialog = ProfileDialog(self._plex)
        self._profile_dialog.set_transient_for(self)
        self._profile_dialog.show()

    def __on_plex_load(self, plex, load_text, status):
        if (status == True):
            self._loading_view.set_text(load_text)
            self._loading_view.set_visible(True)
            self._content_leaflet.set_visible(False)

            self._search_toggle_button.set_sensitive(False)
        else:
            self._loading_view.set_visible(False)
            self._content_leaflet.set_visible(True)

            self._search_toggle_button.set_sensitive(True)

    def __on_show_download_button(self, menu):
        self._download_button.set_visible(True)

    def __custom_css(self):
        screen = Gdk.Screen.get_default()

        css_provider = Gtk.CssProvider()
        css_provider_resource = Gio.File.new_for_uri(
            "resource:///nl/g4d/Girens/plex.css")
        css_provider.load_from_file(css_provider_resource)

        context = Gtk.StyleContext()
        context.add_provider_for_screen(
            screen, css_provider, Gtk.STYLE_PROVIDER_PRIORITY_USER)
