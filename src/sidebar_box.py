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
from .sidebar_server_box import SidebarServerBox

from plexapi.server import PlexServer

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/sidebar_box.ui')
class SidebarBox(Gtk.Box):
    __gtype_name__ = 'sidebar_box'

    __gsignals__ = {
        'home-button-clicked': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'playlists-button-clicked': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'player-button-clicked': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'other-server-selected': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'section-clicked': (GObject.SignalFlags.RUN_FIRST, None, (object,))
    }

    _icons = {
        'movie': 'camera-video-symbolic',
        'show': 'video-display-symbolic',
        'artist': 'audio-x-generic-symbolic'
    }

    _section_list = Gtk.Template.Child()

    def __init__(self, plex, player, **kwargs):
        super().__init__(**kwargs)

        self._plex = plex
        self._player = player

        self._plex.connect('servers-retrieved', self.__on_servers_retrieved)
        # self._plex.connect('sections-retrieved', self.__on_sections_retrieved)
        self._player.connect("media-playing", self.__on_media_playing)

        # self._section_list.connect("row-selected", self.__on_section_clicked)

    def refresh(self):
        thread = threading.Thread(target=self._plex.get_servers)
        thread.daemon = True
        thread.start()

        while self._section_list.get_first_child() is not None:
            self._section_list.remove(self._section_list.get_first_child())

    def __server_changed(self, combo):
        tree_iter = combo.get_active_iter()
        if tree_iter is not None:
            model = combo.get_model()
            server_object, server_string = model[tree_iter][:2]
            if (self._plex._server.friendlyName != server_string):
                self.emit('other-server-selected', server_object)

    def __on_servers_retrieved(self, _plex, servers):
        GLib.idle_add(self.__process_servers, servers)

    def __process_servers(self, servers):
        for server in servers:
            name = ''
            if type(server) == PlexServer:
                name = server.friendlyName
            else:
                name = server.name

            sidebarServerBox = SidebarServerBox()
            self._section_list.append(sidebarServerBox)
            sidebarServerBox.set_title(name)
            sidebarServerBox.set_plex(self._plex)

    def __on_section_clicked(self, listbox, listboxrow):
        if listboxrow is not None:
            child = listboxrow.get_child()
            if child.get_data() is not None:
                self.emit('section-clicked', listboxrow.get_child().get_data())
            else:
                if child.get_title() == 'Home':
                    self.emit('home-button-clicked')
                elif child.get_title() == 'Playlists':
                    self.emit('playlists-button-clicked')
                elif child.get_title() == 'Player':
                    self.emit('player-button-clicked')

    def __on_media_playing(self, player, playing, playqueue_item, playqueue, offset, item):
        if item is not None and item.listType == 'video':
            GLib.idle_add(self._section_player.get_parent().set_visible, True)
        else:
            GLib.idle_add(self._section_player.get_parent().set_visible, False)
