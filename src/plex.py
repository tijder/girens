import time
import os.path
import urllib

from .player import Player

from plexapi.myplex import MyPlexAccount
from plexapi.server import PlexServer
from plexapi.playqueue import PlayQueue
from gi.repository import GObject

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SignalFlags.RUN_FIRST, None, (bool,str)),
        'shows-latest': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'shows-deck': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'download-cover': (GObject.SignalFlags.RUN_FIRST, None, (int,str)),
        'stopped-playing': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'shows-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'servers-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'sections-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
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

    def get_show(self, key):
        show = self._server.fetchItem(int(key))
        episodes = show.episodes()
        self.emit('shows-retrieved',show, episodes)

    def get_servers(self):
        servers = []
        for resource in self._account.resources():
            if (resource.provides == 'server'):
                servers.append(resource)
        self.emit('servers-retrieved', servers)

    def get_sections(self):
        if (self._server is None):
            self.__connect_to_server()
        sections = self._library.sections()
        self.emit('sections-retrieved', sections)

    def download_cover(self, key, thumb):
        url_image = self._server.transcodeImage(thumb, 300, 200)

        path = self.download(url_image, 'thumb_' + str(key))
        self.emit('download-cover', key, path)

    def play_item(self, item):
        playqueue = PlayQueue.create(self._server, item)
        Player(playqueue)
        print('playing stopped')
        self.emit('stopped-playing')

    def mark_as_played(self, item):
        item.markWatched()
        item.reload()
        self.emit('item-retrieved', item)

    def mark_as_unplayed(self, item):
        item.markUnwatched()
        item.reload()
        self.emit('item-retrieved', item)

    def download(self, url_image, prefix):
        path_dir = self._data_dir + '/' + self._server.machineIdentifier
        path = path_dir + '/' + prefix

        if not os.path.exists(path_dir):
            os.makedirs(path_dir)
        if not os.path.exists(path):
            urllib.request.urlretrieve(url_image, path)
            return path
        else:
            return path

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
