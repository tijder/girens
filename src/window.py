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

from gi.repository import Gtk, GLib
from .gi_composites import GtkTemplate

from .sidebar_box import SidebarBox
from .login_view import LoginView
from .discover_view import DiscoverView
from .show_view import ShowView
from .section_view import SectionView
from .search_view import SearchView

from .plex import Plex

import os


@GtkTemplate(ui='/org/gnome/Plex/main_window.ui')
class PlexWindow(Gtk.ApplicationWindow):
    __gtype_name__ = 'PlexWindow'

    _active_view = None

    _login_revealer = GtkTemplate.Child()
    _discover_revealer = GtkTemplate.Child()
    _show_revealer = GtkTemplate.Child()
    _section_revealer = GtkTemplate.Child()
    _search_revealer = GtkTemplate.Child()

    header = GtkTemplate.Child()
    sidebar = GtkTemplate.Child()
    _sidebar_viewport = GtkTemplate.Child()

    _search_bar = GtkTemplate.Child()
    _search_entry = GtkTemplate.Child()

    _back_button = GtkTemplate.Child()
    _search_toggle_button = GtkTemplate.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = Plex(os.environ['XDG_CONFIG_HOME'], os.environ['XDG_CACHE_HOME'])

        self._plex.connect("stopped-playing", self.__on_plex_stopped_playing)

        #self._refresh_button.connect("clicked", self.__on_refresh_clicked)
        self._back_button.connect("clicked", self.__on_back_clicked)

        self._search_toggle_button.connect("toggled", self.__on_search_toggled)
        self._search_entry.connect("search-changed", self.__on_search_changed)
        self._search_entry.connect("stop-search", self.__stop_search)

        self._sidebar_box = SidebarBox(self._plex)
        self._sidebar_box.connect("section-clicked", self.__on_section_clicked)
        self._sidebar_box.connect("home-button-clicked", self.__on_home_clicked)
        self._sidebar_viewport.add(self._sidebar_box)

        self._login_view = LoginView(self._plex)
        self._login_view.connect("login-success", self.__on_login_success)
        self._login_revealer.add(self._login_view)

        self._section_view = SectionView(self._plex)
        self._section_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._section_revealer.add(self._section_view)

        self._search_view = SearchView(self._plex)
        self._search_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._search_revealer.add(self._search_view)

        self._discover_view = DiscoverView(self._plex)
        self._discover_view.connect("view-show-wanted", self.__on_go_to_show_clicked)
        self._discover_revealer.add(self._discover_view)

        self._ShowView = ShowView(self._plex)
        self._show_revealer.add(self._ShowView)

        self.__show_view('login')

    def __show_view(self, view_name):
        self._login_revealer.set_visible(False)
        self._discover_revealer.set_visible(False)
        self._show_revealer.set_visible(False)
        self._section_revealer.set_visible(False)
        self._search_revealer.set_visible(False)

        if view_name == 'login':
            self._login_revealer.set_visible(True)
        elif view_name == 'discover':
            self._discover_revealer.set_visible(True)
        elif view_name == 'show':
            self._show_revealer.set_visible(True)
        elif view_name == 'section':
            self._section_revealer.set_visible(True)
        elif view_name == 'search':
            self._search_revealer.set_visible(True)

        if (view_name != 'search'):
            self._search_toggle_button.set_active(False)

        self._active_view = view_name

    def __on_login_success(self, view, status):
        self._discover_view.refresh()
        self._sidebar_box.refresh()
        self.__show_view('discover')

    def __on_refresh_clicked(self, button):
        self.__refresh_data()

    def __on_home_clicked(self, view):
        self.header.set_visible_child_name("content");
        self._discover_view.refresh()
        self.__show_view('discover')

    def __on_section_clicked(self, view, section):
        self.header.set_visible_child_name("content");
        self._section_view.refresh(section)
        self.__show_view('section')

    def __on_back_clicked(self, button):
        self.header.set_visible_child_name("sidebar");

    def __on_plex_stopped_playing(self, plex):
        GLib.idle_add(self.__refresh_data)

    def __refresh_data(self):
        if(self._active_view == 'discover'):
            self._discover_view.refresh()
        
    def __on_go_to_show_clicked(self, view, key):
        self._ShowView.change_show(key)
        self.__show_view('show')

    def __stop_search(self, search):
        self._search_toggle_button.set_active(False)

    def __on_search_toggled(self, toggle):
        self._search_bar.set_search_mode(toggle.get_active())

    def __on_search_changed(self, entry):
        if (entry.get_text() != "" and len(entry.get_text()) >= 3):
            self.header.set_visible_child_name("content");
            self._search_view.refresh(entry.get_text())
            self.__show_view('search')
