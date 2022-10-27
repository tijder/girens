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

from gi.repository import Gtk, GLib, GObject


import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/login_view.ui')
class LoginView(Gtk.Box):
    __gtype_name__ = 'login_view'

    __gsignals__ = {
        'login-success': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'login-failed': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'login-not-found': (GObject.SignalFlags.RUN_FIRST, None, ())
    }

    _loading = False
    _try_token = False

    _username_entry = Gtk.Template.Child()
    _password_entry = Gtk.Template.Child()
    _connect_user_button = Gtk.Template.Child()

    _server_url_entry = Gtk.Template.Child()
    _server_token_entry = Gtk.Template.Child()
    _connect_url_button = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

        #self._password_entry.connect("changed", self.__entry_changed)
        #self._username_entry.connect("changed", self.__entry_changed)
        self._connect_url_button.connect("clicked", self.__on_connect_url_clicked)
        self._connect_user_button.connect("clicked", self.__on_login_clicked)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("login-status", self.__plex_login_status)

    def try_login(self):
        if (self._plex.has_token()):
            self.__login_with_token()
        elif (self._plex.has_url()):
            self._try_token = True
            self._plex.login_with_url(self._plex._server_url, self._plex._server_token)
        else:
            self.emit('login-not-found')

    def __plex_login_status(self, plex, success, message):
        GLib.idle_add(self.__plex_login_status_process, success, message)

    def __plex_login_status_process(self, success, message):
        self._loading = False
        self._connect_user_button.set_sensitive(not self._loading)
        self._connect_url_button.set_sensitive(not self._loading)
        self._server_url_entry.set_can_focus(True)
        self._username_entry.set_can_focus(True)
        self._password_entry.set_can_focus(True)
        if(success):
            self.__show_correct_login()
            self.emit('login-success',True)
        else:
            if (self._try_token == True):
                self.emit('login-failed')
            self.__show_incorrect_login()

    def __entry_changed(self, entry):
        #self._password_entry.set_icon_from_icon_name(Gtk.EntryIconPosition(1), None)
        pass

    def __login_with_token(self):
        self._try_token = True
        self._loading = True
        self.__show_loading()

        thread = threading.Thread(target=self._plex.login_token, args=(self._plex._token,))
        thread.daemon = True
        thread.start()

    def __on_login_clicked(self, button):
        if (self._loading == False):
            self._loading = True
            self.__show_loading()

            username = self._username_entry.get_text()
            password = self._password_entry.get_text()
            thread = threading.Thread(target=self._plex.login, args=(username, password))
            thread.daemon = True
            thread.start()

    def __on_connect_url_clicked(self, button):
        if (self._loading == False):
            self._loading = True
            self.__show_loading()

            url = self._server_url_entry.get_text()
            token = self._server_token_entry.get_text()
            thread = threading.Thread(target=self._plex.login_with_url, args=(url, token))
            thread.daemon = True
            thread.start()

    def __show_loading(self):
        #self._password_entry.set_progress_fraction(0.10)
        self._connect_user_button.set_sensitive(not self._loading)
        self._connect_url_button.set_sensitive(not self._loading)
        self._server_url_entry.set_can_focus(False)
        self._username_entry.set_can_focus(False)
        self._password_entry.set_can_focus(False)

    def __show_correct_login(self):
        #self._password_entry.set_progress_fraction(1.00)
        pass

    def __show_incorrect_login(self):
        #self._password_entry.set_progress_fraction(0.00)
        #self._password_entry.set_icon_from_icon_name(Gtk.EntryIconPosition(1), 'dialog-error')
        #self._server_url_entry.set_icon_from_icon_name(Gtk.EntryIconPosition(1), 'dialog-error')
        pass
        
