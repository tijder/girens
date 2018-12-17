import time
import os.path

from plexapi.myplex import MyPlexAccount
from plexapi.server import PlexServer
from gi.repository import GObject

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SignalFlags.RUN_FIRST, None, (bool,str)),
        'shows-latest': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'shows-deck': (GObject.SignalFlags.RUN_FIRST, None, (object,))
    }

    _token = None
    _server_url = None
    _server = None
    _account = None
    _library = None

    def __init__(self, config_dir, data_dir, **kwargs):
        super().__init__(**kwargs)
        self._config_dir = config_dir
        self._data_dir = data_dir
        if(os.path.isfile(self._config_dir + '/config')):
           with open(self._config_dir + '/config', 'r') as file:
               lines = file.readlines()
               self._token = lines[0].split()[0]
               if (len(lines) >= 2):
                   self._server_url = lines[1].split()[0]

    def has_token(self):
        return self._token is not None

    def login_token(self, token):
        try:
            self._account = MyPlexAccount(token=token)
            self.__save_login(self._account._token)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def login(self, username, password):
        try:
            self._account = MyPlexAccount(username, password)
            self.__save_login(self._account._token)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def __save_login(self, token):
        with open(self._config_dir + '/config', 'a') as file:
                file.write(token)

    def get_latest(self):
        if (self._server is None):
            self.__connect_to_server()
            
        latest = self._library.recentlyAdded()
        self.emit('shows-latest',latest)

    def get_deck(self):
        if (self._server is None):
            self.__connect_to_server()

        deck = self._library.onDeck()
        self.emit('shows-deck',deck)

    def __connect_to_server(self):
        if (self._server_url is not None):
            try:
                self._server = PlexServer(self._server_url, self._token)
                self._library = self._server.library
            except:
                print('custom url connection failed')
        else:
            for resource in self._account.resources():
                if (resource.provides == 'server'):
                    try:
                        print(resource)
                        print(self._account.secure)
                        print(resource.connections)
                        self._server = resource.connect(ssl=self._account.secure)
                        self._library = self._server.library
                        break
                    except:
                        print('connection failed')
