# cover_box.py
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
from gi.repository import Gtk, GLib, GObject


from .section_grid import SectionGrid

from plexapi.server import PlexServer

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/sidebar_server_box.ui')
class SidebarServerBox(Gtk.Box):
    __gtype_name__ = 'sidebar_server_box'

    _icons = {
        'movie': 'camera-video-symbolic',
        'show': 'video-display-symbolic',
        'artist': 'audio-x-generic-symbolic'
    }

    _label = Gtk.Template.Child()
    _section_list = Gtk.Template.Child()
    _spinner = Gtk.Template.Child()

    _plex = None

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_plex(self, plex):
        self._plex = plex

        self._plex.connect('sections-retrieved', self.__on_sections_retrieved)

        self._set_loading(True)
        thread = threading.Thread(target=self._plex.get_sections)
        thread.daemon = True
        thread.start()

    def _set_loading(self, loaded):
        self._spinner.set_spinning(loaded)

    def set_title(self, title):
        self._label.set_label(title)

    def __on_sections_retrieved(self, _plex, sections):
        GLib.idle_add(self.__process_section, sections)

    def __process_section(self, sections):
        self._section_player = SectionGrid()
        self._section_player.set_title('Player')
        self._section_player.set_custom_title(_('Player'))
        self._section_player.set_from_icon_name('media-playback-start-symbolic')
        self._section_player.set_action_target_value(GLib.Variant.new_string("Home"))
        self._section_list.append(self._section_player)
        self._section_player.get_parent().set_visible(False)
        section_grid = SectionGrid()
        section_grid.set_title('Home')
        section_grid.set_custom_title(_('Home'))
        section_grid.set_from_icon_name('user-home-symbolic')
        self._section_list.append(section_grid)
        section_grid = SectionGrid()
        section_grid.set_title('Playlists')
        section_grid.set_custom_title(_('Playlists'))
        section_grid.set_from_icon_name('view-list-symbolic')
        self._section_list.append(section_grid)
        for section in sections:
            if (section.type == 'movie' or section.type == 'show' or section.type == 'artist'):
                section_grid = SectionGrid()
                section_grid.set_title(section.title)
                section_grid.set_data(section)
                section_grid.set_from_icon_name(self._icons[section.type])
                self._section_list.append(section_grid)
        self.select_home()
        self._set_loading(False)
        self._section_list.set_visible(True)

    def unselect_all(self):
        self._section_list.unselect_all()

    def select_player(self):
        self._section_list.select_row(self._section_list.get_row_at_index(0))

    def select_home(self):
        self._section_list.select_row(self._section_list.get_row_at_index(1))

