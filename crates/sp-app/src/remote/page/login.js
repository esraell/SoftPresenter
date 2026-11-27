// Signs this device in with the password; the computer answers with a cookie this page can't
// read, and the device is remembered.
"use strict";
'use strict';
// The remote page's words in the app's languages: built by translation-review/es/build-remote-table.js from the app's
// own tables (crates/sp-app/locales) and remote-map.json. Don't edit by hand: rebuild. Read by remote-lang.js.
const REMOTE_WORDS = {
 "es": {
  "exact": {
   "A Bible passage…": "Un pasaje de la Biblia…",
   "A header…": "Un encabezado…",
   "A Library Folder…": "Una carpeta de biblioteca…",
   "A media playlist": "Una lista multimedia",
   "A picture or video": "Una imagen o un vídeo",
   "A placeholder…": "Un hueco…",
   "A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the order they come.": "Una lista es el orden de un culto: canciones, pasajes de la Biblia, imágenes y vídeos, en el orden en que van.",
   "A question is waiting: click to answer it": "Hay una pregunta esperando: haz clic para responderla",
   "a service, in order": "un culto, en orden",
   "A small tag beside each button with its key. Also in Settings › Keys.": "Una etiqueta pequeña junto a cada botón con su tecla. También en Ajustes › Teclas.",
   "A Song Bundle…": "Un paquete de canciones…",
   "A song…": "Una canción…",
   "A–Z": "A–Z",
   "Actions": "Acciones",
   "Add a Bible passage": "Añadir un pasaje de la Biblia",
   "Add a header": "Añadir un encabezado",
   "Add a song": "Añadir una canción",
   "Add a track to this playlist": "Añade una pista a esta lista",
   "Add pictures or videos": "Añadir imágenes o vídeos",
   "Add pictures or videos to show behind the words": "Añade imágenes o vídeos para mostrarlos detrás de la letra",
   "Add Slide": "Añadir diapositiva",
   "Add the Sample Backgrounds": "Añadir los fondos de ejemplo",
   "Add to playlist": "Añadir a la lista",
   "Add to Playlist": "Añadir a una lista",
   "Adding music is done at the computer for now.": "Por ahora, la música se añade en el ordenador.",
   "Adding pictures and videos from here comes with the next step. Add them at the computer for now.": "Añadir imágenes y vídeos desde aquí llegará en el próximo paso. Por ahora, añádelos en el ordenador.",
   "Advanced": {
    "in": [
     [
      ".st-navg",
      "Avanzado"
     ]
    ],
    "else": "Más"
   },
   "All Media": "Todo",
   "Announce": "Anuncios",
   "Arrangement": "Arreglo: reproducir los grupos de la canción en otro orden",
   "As listed": "Como está",
   "Audience": "Público",
   "Audience screens on or off (⇧⌘1)": "Pantallas del público: encender o apagar (⇧⌘1)",
   "Audience screens, stage screens, on or off": "Pantallas del público y del escenario, encender o apagar",
   "Audio": "Audio",
   "Audio playlist": "lista de audio",
   "Back 15 seconds": "Retroceder 15 segundos",
   "Bible": "Biblia",
   "Bible, too (Bold while editing a slide)": "También Biblia (Negrita al editar una diapositiva)",
   "Blinds": "Persianas",
   "Cancel": "Cancelar",
   "Changing songs, playlists and settings is done at the computer for now. Running the show works from here.": "Por ahora, las canciones, las listas y los ajustes se cambian en el ordenador. Llevar la presentación sí se puede desde aquí.",
   "Changing things": "Cambios",
   "Checkerboard": "Tablero",
   "Choose": "Elegir",
   "Choose a library or playlist (⌘P)": "Elegir una lista o una biblioteca (⌘P)",
   "Choose a song…": "Elegir una canción…",
   "Choosing from the menu bar is done at the computer, not from the remote page.": "Elegir en la barra de menús se hace en el ordenador, no desde la página remota.",
   "Choosing the stage layouts": "Elegir los diseños del escenario",
   "CLEAR": "DESPEJADA",
   "Clear All": "Borrar todo",
   "Clear everything": "Borrar todo",
   "Clear it": "Quitarlo",
   "Clear the slide, media, props, audio or messages: the layer keys under the preview": "Quitar la diapositiva, lo multimedia, los gráficos, el audio o los mensajes: las teclas de capa bajo la vista previa",
   "Clearing": "Borrar",
   "Click to show it": "Haz clic para mostrarlo",
   "Clock": "Reloj",
   "Close": "Cerrar",
   "comes to the remote page with the next step. Use the computer for now.": "llegará a esta página en el próximo paso. Por ahora, usa el ordenador.",
   "Connect": "Conectar",
   "Connected": "Conectado",
   "Connecting…": "Conectando…",
   "Continuous": "Continuo",
   "Copy": "Copiar",
   "Copy Style": "Copiar estilo",
   "Copy, cut, paste the picked slides (objects in Edit)": "Copiar, cortar y pegar las diapositivas elegidas (objetos en Editar)",
   "Could not sign in.": "No se pudo iniciar sesión.",
   "Crossfade": "Fundido",
   "Cut": {
    "in": [
     [
      "#foot",
      "Corte"
     ]
    ],
    "else": "Cortar"
   },
   "Delete": {
    "in": [
     [
      ".ui-keys",
      "Supr"
     ]
    ],
    "else": "Eliminar"
   },
   "Delete Song from Library…": "Eliminar canción de la biblioteca…",
   "Delete what is picked: what Undo brings back goes at once (playlist items, slides, a media playlist’s items); songs in a library, pictures in the bin and screens ask first": "Eliminar lo elegido: lo que Deshacer recupera se elimina al momento (elementos de la lista, diapositivas, elementos de una lista multimedia); las canciones de una biblioteca, las imágenes de la mediateca y las pantallas preguntan antes",
   "Delete…": "Eliminar…",
   "Details": "Detalles: el autor, el editor y el número CCLI de la canción",
   "Disable": "Desactivar diapositiva",
   "Disconnected at the computer: click to connect again": "Desconectado desde el ordenador: haz clic para volver a conectar",
   "Dissolve": "Fundido",
   "Done": "Hecho",
   "Door": "Puerta",
   "Drag it into a playlist": "Arrástralo a una lista",
   "Drag it into a playlist or a library": "Arrástralo a una lista o a una biblioteca",
   "Duplicate": "Duplicar",
   "Edit": {
    "in": [
     [
      "#modes",
      "Editar"
     ]
    ],
    "else": "Editar"
   },
   "Edit this slide": "Editar esta diapositiva",
   "Edit Words": "Editar letra",
   "Every key (press ?)": "Todas las teclas (pulsa ?)",
   "Every other transition": "Todas las demás transiciones, y lo que tardan",
   "every song you keep": "todas tus canciones",
   "Fade": "Desvanecer",
   "Fade Black": "Desvanecer a negro",
   "Fade White": "Desvanecer a blanco",
   "Fill with Song…": "Rellenar con una canción…",
   "Filter": "Filtrar",
   "Find a song, a line or a verse": "Buscar una canción, una línea o un versículo",
   "Find a song, a line, or a verse": "Buscar canción, línea o versículo",
   "First slide of the song, or verse in Bible": "Primera diapositiva de la canción, o primer versículo en Biblia",
   "Flip": "Girar",
   "Fly In": "Entrar volando",
   "Forward 15 seconds": "Adelantar 15 segundos",
   "Go to Next": "Pasar a la siguiente",
   "Go to the slide on screen": "Ir a la diapositiva que está en pantalla",
   "Going places": "Ir a sitios",
   "Grid": "Cuadrícula",
   "Group": "Grupo",
   "Hide": "Ocultar",
   "Hide the media strip": "Ocultar la tira multimedia",
   "Hot Key…": "Tecla rápida…",
   "How long one track fades into the next": "Cuánto tarda una pista en fundirse con la siguiente",
   "Import Songs": "Importar canciones",
   "Import songs or slides": "Importar canciones o diapositivas",
   "Import…": "Importar…",
   "In Edit: a new text box, duplicate, pick everything": "En Editar: un cuadro de texto nuevo, duplicar, elegirlo todo",
   "In Edit: nudge the picked object (Shift for one pixel)": "En Editar: mover el objeto elegido (con Mayús, un píxel)",
   "Iris": "Iris",
   "is empty": "está vacía",
   "Jump to a group (C for the chorus), or a slide’s own hot key": "Saltar a un grupo (C para el coro), o a la tecla propia de una diapositiva",
   "Keep playing until stopped: after the last track, start again from the first (with Play through off, play this track again)": "Seguir sonando hasta que lo pares: tras la última pista, volver a empezar por la primera (con Continuo apagado, repetir esta pista)",
   "Keys": "Teclas",
   "Label": "Etiqueta",
   "Last slide of the song, or verse in Bible": "Última diapositiva de la canción, o último versículo en Biblia",
   "Library": "Biblioteca",
   "Longest": "Más largas",
   "Looking up the Bible comes to the remote page with the next step. Use the Bible page at the computer for now.": "Buscar en la Biblia llegará a esta página en el próximo paso. Por ahora, usa la página de la Biblia en el ordenador.",
   "Loop": {
    "in": [
     [
      ".lv-audio",
      "Repetir"
     ]
    ],
    "else": "En bucle"
   },
   "Macros": "Macros",
   "Media": "Multimedia",
   "Media Playlists": "Listas multimedia",
   "Menus and windows": "Menús y ventanas",
   "Messages": "Mensajes",
   "More": "Más",
   "Move": "Moverse",
   "Move Down": "Bajar",
   "Move In": "Entrar",
   "Move Up": "Subir",
   "Mute": "Silenciar",
   "New": "Temporizador nuevo",
   "New library": "Biblioteca nueva",
   "New Media Playlist…": "Lista multimedia nueva…",
   "New playlist": "Lista nueva",
   "New song": "Canción nueva",
   "Next": "Siguiente",
   "Next at the end of a song goes on to the next song": "Al final de una canción, Siguiente pasa a la siguiente canción",
   "Next slide": "Diapositiva siguiente",
   "Next track": "Pista siguiente",
   "No song is open": "No hay ninguna canción abierta",
   "No songs here yet. Make one, import some, or drag songs onto this library’s name.": "Aún no hay canciones aquí. Crea una, importa algunas o arrastra canciones sobre el nombre de esta biblioteca.",
   "No tracks yet. Add music files from this computer.": "Aún no hay pistas. Añade archivos de música de este ordenador.",
   "Not connected to SoftPresenter.": "No hay conexión con SoftPresenter.",
   "Notes": "Notas",
   "Nothing found": "No se encontró nada",
   "Nothing showing": "No se muestra nada",
   "Nothing to play": "Nada que reproducir",
   "Nothing to redo": "Nada que rehacer",
   "Nothing to tell you. Trouble, questions, and what happens out of sight (added to another playlist, a file read or written) come here.": "Nada que contarte. Aquí llegan los problemas, las preguntas y lo que pasa fuera de la vista (algo añadido a otra lista, un archivo leído o guardado).",
   "Nothing to undo": "Nada que deshacer",
   "Notices": "Avisos",
   "Off": {
    "in": [
     [
      ".ui-sw",
      "No"
     ],
     [
      "#outAudience, #outStage",
      "No"
     ]
    ],
    "else": "No"
   },
   "ON": "SÍ",
   "ON SCREEN": "EN PANTALLA",
   "Open": "Abrir",
   "Open a group of choices; go back": "Abrir un grupo de opciones; volver",
   "Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive": "Abrir Temporizadores, Mensajes, Gráficos, Macros, Escenario, Audio, VoxLive",
   "Or use the sample backgrounds": "O usa los fondos de ejemplo",
   "Order": "Orden",
   "Password": "Contraseña",
   "Paste": "Pegar",
   "Paste Style": "Pegar estilo",
   "Pause": "Pausa",
   "Pick several in the left column: grow the pick, or pick them all": "Elegir varios en la columna izquierda: ampliar la selección o elegirlos todos",
   "Picture": "Imagen",
   "Placeholder": "Hueco",
   "Play": "Reproducir",
   "Play on Announcements Layer": "Reproducir en la capa de anuncios",
   "Play the tracks in a random order": "Reproducir las pistas en orden aleatorio",
   "Play through": "Continuo",
   "Playing": "Sonando",
   "Playlist": "Lista",
   "Playlists": "Listas",
   "Playlists and libraries": "Listas y bibliotecas",
   "Press / to search from anywhere": "Pulsa / para buscar desde cualquier sitio",
   "Prev": "Ant.",
   "Previous": "Anterior",
   "Previous slide": "Diapositiva anterior",
   "Props": "Gráficos",
   "Push": "Empujar",
   "Ready": "Listo",
   "Reconnecting…": "Reconectando…",
   "Redo": "Rehacer",
   "Remote Control": "Control remoto",
   "Remove from Playlist": "Quitar de la lista",
   "Rename": "Renombrar",
   "Reset": "Reiniciar",
   "Reveal": "Revelar",
   "Running the slides": "Las diapositivas",
   "s": "s",
   "Scaling": "Tamaño",
   "Service": "Culto",
   "Service (⌥S)": "Culto (⌥S)",
   "Service, Bible (ready for a reference), Edit (in the app ⌘1 ⌘2 ⌘3; a browser keeps those)": "Culto, Biblia (lista para una referencia), Editar (en el programa ⌘1 ⌘2 ⌘3; un navegador se los queda)",
   "Settings": "Ajustes",
   "Settings are changed at the computer.": "Los ajustes se cambian en el ordenador.",
   "Show": "Mostrar",
   "Show a message": "Mostrar un mensaje",
   "Show keys on buttons": "Teclas en los botones",
   "Show that picture or video": "Mostrar esa imagen o ese vídeo",
   "Shuffle": "Aleatorio",
   "Sign In": "Iniciar sesión",
   "Sign Out of This Device…": "Cerrar sesión en este dispositivo…",
   "Signing in…": "Iniciando sesión…",
   "Slide": "Diapositiva",
   "Slide size": "Tamaño de las diapositivas",
   "Slides…": "Diapositivas…",
   "SoftPresenter Remote Control": "Control remoto de SoftPresenter",
   "Something went wrong: click to see": "Algo ha fallado: haz clic para verlo",
   "Song": "Canción",
   "Song Files…": "Archivos de canciones…",
   "Song libraries": "Bibliotecas de canciones",
   "Split": "Dividir",
   "Stage": "Escenario",
   "Stage screens on or off (⇧⌘2)": "Pantallas del escenario: encender o apagar (⇧⌘2)",
   "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track": "Iniciar ese temporizador, mostrar ese mensaje o gráfico, ejecutar esa macro, poner el escenario en ese diseño, reproducir esa pista",
   "Swap": "Intercambiar",
   "Table": "Tabla",
   "Template": "Plantilla",
   "The car with the lights on, please": "El coche con las luces encendidas, por favor",
   "The computer didn't answer.": "El ordenador no respondió.",
   "The cued slide or verse goes on screen (after a search, or ↑ ↓)": "La diapositiva o el versículo marcado pasa a pantalla (después de una búsqueda, o con ↑ ↓)",
   "The item above or below, cued at its first slide; in Bible, the verse before or after": "El elemento de arriba o de abajo; en Biblia, elegir el versículo anterior o siguiente",
   "The keys listed in Settings › Keys do what it says there. Any other key can be added.": "Las teclas de la lista de Ajustes › Teclas hacen lo que allí se indica. Se puede añadir cualquier otra tecla.",
   "The live tools": "Las herramientas en vivo",
   "The menu for the slide on screen": "El menú de la diapositiva que está en pantalla",
   "then 1–9": "luego 1 … 9",
   "This device is remembered once it signs in, until it is forgotten in Settings › Phone remote on the computer.": "Este dispositivo se recuerda en cuanto inicia sesión, hasta que lo olvides en Ajustes › Mando del móvil en el ordenador.",
   "This library has no songs yet.": "Esta biblioteca aún no tiene canciones.",
   "This list": "Esta lista (al pie tiene Teclas en los botones)",
   "This sample only has": "Esta muestra solo tiene",
   "Timers": "Temporizadores",
   "track": "Pista",
   "Transition": "Transición",
   "Typing in a box: bold, italic, underlined, for the words selected (or the whole box)": "Al escribir en un cuadro: negrita, cursiva, subrayado, para el texto elegido (o todo el cuadro)",
   "Undo": "Deshacer",
   "Undo, redo": "Deshacer, rehacer",
   "Volume": "Volumen",
   "VoxLive": "VoxLive",
   "Waiting for SoftPresenter": "Esperando a SoftPresenter",
   "Waiting for SoftPresenter to answer. Try again in a moment.": "Esperando la respuesta de SoftPresenter. Vuelve a intentarlo en un momento.",
   "When a track ends, go on to the next": "Al terminar una pista, pasar a la siguiente",
   "Wipe": "Barrido",
   "With nothing on screen, move the cue; Space or Page Down or Enter puts it on": "Sin nada en pantalla, mueve la marca; Espacio, Av Pág o Intro la pone en pantalla",
   "Words": {
    "in": [
     [
      ".ui-sheet",
      "Texto"
     ]
    ],
    "else": "Letra"
   },
   "Words from Text…": "Letra desde texto…",
   "Words once…": "Texto una vez…",
   "Words only": "Solo letra",
   "Zoom In": "Acercar",
   "Letting a phone, iPad or Mac show its screen is decided at the computer, not from the remote page.": "Dejar que un teléfono, iPad o Mac muestre su pantalla se decide en el ordenador, no desde la página remota.",
   "There is no track to play. Add music at the computer.": "No hay ninguna pista para reproducir. Añade música en el ordenador."
  },
  "patterns": [
   {
    "re": "^(.+) \\(⌥(\\d), then 1 to 9 for what is in it\\)$",
    "to": "{1t} (⌥{2}, luego del 1 al 9 para lo que contiene)",
    "key": "page.tool_tab.hint"
   },
   {
    "re": "^Running SoftPresenter as (.+)$",
    "to": "Usando SoftPresenter como {1}",
    "key": "page.conn.as"
   },
   {
    "re": "^Notices: (\\d+) need you$",
    "to": "Avisos: {1} te necesitan",
    "key": "page.notices.many"
   },
   {
    "re": "^Counts down from (.+?)$",
    "to": "Cuenta atrás desde {1}",
    "key": "tools.timer.down_from"
   },
   {
    "re": "^(.+) (\\d+(?:\\.\\d+)?)s$",
    "to": "{1t} {2} s",
    "key": "page.transition.time"
   },
   {
    "re": "^Notices: 1 needs you$",
    "to": "Avisos: 1 te necesita",
    "key": "page.notices.one"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?) s$",
    "to": "{1} s",
    "key": "common.seconds_unit"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?)s$",
    "to": "{1} s",
    "key": "common.seconds_unit"
   },
   {
    "re": "^Bible · (.+)$",
    "to": "Biblia · {1}",
    "key": "page.find.bible_tag"
   },
   {
    "re": "^Add to (.+?)$",
    "to": "Añadir a {1}",
    "key": "left.song.add_to"
   },
   {
    "re": "^(.+?) tracks$",
    "to": "{1} pistas",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Song · (.+)$",
    "to": "Canción · {1}",
    "key": "find.tag.song_group"
   },
   {
    "re": "^(.+?) items$",
    "to": "{1} elementos",
    "key": "media.items"
   },
   {
    "re": "^(.+?) songs$",
    "to": "{1} canciones",
    "key": "integrations.songbooks.songs"
   },
   {
    "re": "^(.+?) track$",
    "to": "{1} pista",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Start (.+?)$",
    "to": "Iniciar {1}",
    "key": "tools.steps.timer_start"
   }
  ],
  "keys": {
   "Shift": "Mayús"
  },
  "format": {
   "decimal": ",",
   "group": ".",
   "group_from": 5,
   "quote_open": "“",
   "quote_close": "”"
  }
 },
 "ru": {
  "exact": {
   "A Bible passage…": "Отрывок из Библии…",
   "A header…": "Заголовок…",
   "A Library Folder…": "Папка библиотеки…",
   "A media playlist": "Медиасписок",
   "A picture or video": "Картинка или видео",
   "A placeholder…": "Пустое место…",
   "A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the order they come.": "Список — это порядок служения: песни, отрывки из Библии, картинки и видео, в том порядке, в каком они идут.",
   "A question is waiting: click to answer it": "Есть вопрос: нажмите, чтобы ответить",
   "a service, in order": "служение, по порядку",
   "A small tag beside each button with its key. Also in Settings › Keys.": "Маленькая метка с клавишей возле каждой кнопки. Также в разделе Настройки › Клавиши.",
   "A Song Bundle…": "Пакет песен…",
   "A song…": "Песня…",
   "A–Z": "А–Я",
   "Actions": "Действия",
   "Add a Bible passage": "Добавить отрывок из Библии",
   "Add a header": "Добавить заголовок",
   "Add a song": "Добавить песню",
   "Add a track to this playlist": "Добавьте трек в этот список",
   "Add pictures or videos": "Добавить картинки или видео",
   "Add pictures or videos to show behind the words": "Добавьте картинки или видео, чтобы показывать их за словами",
   "Add Slide": "Добавить слайд",
   "Add the Sample Backgrounds": "Добавить образцы фонов",
   "Add to playlist": "Добавить в список",
   "Add to Playlist": "Добавить в список",
   "Adding music is done at the computer for now.": "Музыка пока добавляется за компьютером.",
   "Adding pictures and videos from here comes with the next step. Add them at the computer for now.": "Добавлять картинки и видео отсюда можно будет на следующем этапе. Пока добавляйте их за компьютером.",
   "Advanced": {
    "in": [
     [
      ".st-navg",
      "Дополнительно"
     ]
    ],
    "else": "Другие"
   },
   "All Media": "Все медиа",
   "Announce": "Объявления",
   "Arrangement": "Порядок исполнения: петь части песни в другом порядке",
   "As listed": "Как в библиотеке",
   "Audience": "Зал",
   "Audience screens on or off (⇧⌘1)": "Экраны для зала: включить или выключить (⇧⌘1)",
   "Audience screens, stage screens, on or off": "Экраны для зала, экраны для сцены: вкл. или выкл.",
   "Audio": "Аудио",
   "Audio playlist": "Аудиосписок",
   "Back 15 seconds": "Назад на 15 секунд",
   "Bible": "Библия",
   "Bible, too (Bold while editing a slide)": "Тоже Библия (при правке слайда — жирный)",
   "Blinds": "Жалюзи",
   "Cancel": "Отмена",
   "Changing songs, playlists and settings is done at the computer for now. Running the show works from here.": "Песни, списки и настройки пока меняются за компьютером. Вести показ можно отсюда.",
   "Changing things": "Изменения",
   "Checkerboard": "Шахматы",
   "Choose": "Выбрать",
   "Choose a library or playlist (⌘P)": "Выберите список или библиотеку (⌘P)",
   "Choose a song…": "Выбрать песню…",
   "Choosing from the menu bar is done at the computer, not from the remote page.": "Выбор в строке меню делается за компьютером, а не со страницы пульта.",
   "Choosing the stage layouts": "Выбор макетов сцены",
   "CLEAR": "ПУСТО",
   "Clear All": "Очистить всё",
   "Clear everything": "Очистить всё",
   "Clear it": "Убрать",
   "Clear the slide, media, props, audio or messages: the layer keys under the preview": "Убрать слайд, медиа, накладки, аудио или сообщения: кнопки слоёв под предпросмотром",
   "Clearing": "Очистка",
   "Click to show it": "Нажмите, чтобы показать",
   "Clock": "Часы",
   "Close": "Закрыть",
   "comes to the remote page with the next step. Use the computer for now.": "появится на странице пульта на следующем этапе. Пока пользуйтесь компьютером.",
   "Connect": "Подключить",
   "Connected": "Подключено",
   "Connecting…": "Подключение…",
   "Continuous": "Подряд",
   "Copy": "Копировать",
   "Copy Style": "Копировать стиль",
   "Copy, cut, paste the picked slides (objects in Edit)": "Копировать, вырезать, вставить выбранные слайды (в Редакторе — объекты)",
   "Could not sign in.": "Не удалось войти.",
   "Crossfade": "ПЕРЕХОД",
   "Cut": {
    "in": [
     [
      "#foot",
      "Сразу"
     ]
    ],
    "else": "Вырезать"
   },
   "Delete": {
    "in": [
     [
      ".ui-keys",
      "Delete"
     ]
    ],
    "else": "Удалить"
   },
   "Delete Song from Library…": "Удалить песню из библиотеки…",
   "Delete what is picked: what Undo brings back goes at once (playlist items, slides, a media playlist’s items); songs in a library, pictures in the bin and screens ask first": "Удалить выбранное: то, что возвращает «Отменить», удаляется сразу (пункты списка, слайды, пункты медиасписка); песни в библиотеке, картинки в корзине и экраны сначала спрашивают",
   "Delete…": "Удалить…",
   "Details": "Сведения: автор песни, издатель и номер CCLI",
   "Disable": "Отключить слайд",
   "Disconnected at the computer: click to connect again": "Отключено на компьютере: нажмите, чтобы подключиться снова",
   "Dissolve": "Плавно",
   "Done": "Готово",
   "Door": "Двери",
   "Drag it into a playlist": "Перетащите в список",
   "Drag it into a playlist or a library": "Перетащите в список или в библиотеку",
   "Duplicate": "Дублировать",
   "Edit": {
    "in": [
     [
      "#modes",
      "Редактор"
     ]
    ],
    "else": "Изменить"
   },
   "Edit this slide": "Изменить этот слайд",
   "Edit Words": "Изменить слова",
   "Every key (press ?)": "Все клавиши (нажмите ?)",
   "Every other transition": "Все остальные переходы и их длительность",
   "every song you keep": "все ваши песни",
   "Fade": "Затухание",
   "Fade Black": "Через чёрный",
   "Fade White": "Через белый",
   "Fill with Song…": "Заполнить песней…",
   "Filter": "Фильтр",
   "Find a song, a line or a verse": "Найти песню, строку или стих",
   "Find a song, a line, or a verse": "Найти песню, строку или стих",
   "First slide of the song, or verse in Bible": "Первый слайд песни или первый стих в Библии",
   "Flip": "Переворот",
   "Fly In": "Влёт",
   "Forward 15 seconds": "Вперёд на 15 секунд",
   "Go to Next": "Переход дальше",
   "Go to the slide on screen": "Перейти к слайду, который на экране",
   "Going places": "Переходы",
   "Grid": "Сетка",
   "Group": "Часть",
   "Hide": "Скрыть",
   "Hide the media strip": "Скрыть полосу медиа",
   "Hot Key…": "Горячая клавиша…",
   "How long one track fades into the next": "Сколько длится плавный переход одного трека в следующий",
   "Import Songs": "Импорт песен",
   "Import songs or slides": "Импорт песен или слайдов",
   "Import…": "Импорт…",
   "In Edit: a new text box, duplicate, pick everything": "В Редакторе: новый текстовый блок, дублировать, выбрать всё",
   "In Edit: nudge the picked object (Shift for one pixel)": "В Редакторе: сдвинуть выбранный объект (с Shift — на один пиксель)",
   "Iris": "Диафрагма",
   "is empty": "— пустой список",
   "Jump to a group (C for the chorus), or a slide’s own hot key": "Перейти к части (C — припев) или по собственной горячей клавише слайда",
   "Keep playing until stopped: after the last track, start again from the first (with Play through off, play this track again)": "Играть, пока не остановят: после последнего трека начать снова с первого (если «Подряд» выключено — повторять этот трек)",
   "Keys": "Клавиши",
   "Label": "Метка",
   "Last slide of the song, or verse in Bible": "Последний слайд песни или последний стих в Библии",
   "Library": "Библиотека",
   "Longest": "Самые длинные",
   "Looking up the Bible comes to the remote page with the next step. Use the Bible page at the computer for now.": "Поиск по Библии появится на странице пульта на следующем этапе. Пока пользуйтесь страницей Библии на компьютере.",
   "Loop": {
    "in": [
     [
      ".lv-audio",
      "Повтор"
     ]
    ],
    "else": "По кругу"
   },
   "Macros": "Макросы",
   "Media": "Медиа",
   "Media Playlists": "Медиасписки",
   "Menus and windows": "Меню и окна",
   "Messages": "Сообщения",
   "More": "Ещё",
   "Move": "Перемещаться",
   "Move Down": "Переместить вниз",
   "Move In": "Въезд",
   "Move Up": "Переместить вверх",
   "Mute": "Выключить звук",
   "New": "Новый таймер",
   "New library": "Новая библиотека",
   "New Media Playlist…": "Новый медиасписок…",
   "New playlist": "Новый список",
   "New song": "Новая песня",
   "Next": "Далее",
   "Next at the end of a song goes on to the next song": "«Далее» в конце песни переходит к следующей песне",
   "Next slide": "Следующий слайд",
   "Next track": "Следующий трек",
   "No song is open": "Песня не открыта",
   "No songs here yet. Make one, import some, or drag songs onto this library’s name.": "Здесь пока нет песен. Создайте песню, импортируйте или перетащите песни на название этой библиотеки.",
   "No tracks yet. Add music files from this computer.": "Треков пока нет. Добавьте музыкальные файлы с этого компьютера.",
   "Not connected to SoftPresenter.": "Нет связи с SoftPresenter.",
   "Notes": "Заметки",
   "Nothing found": "Ничего не найдено",
   "Nothing showing": "Ничего не показано",
   "Nothing to play": "НЕЧЕГО ИГРАТЬ",
   "Nothing to redo": "Нечего повторять",
   "Nothing to tell you. Trouble, questions, and what happens out of sight (added to another playlist, a file read or written) come here.": "Пока сообщать нечего. Сюда приходят неполадки, вопросы и то, что происходит вне поля зрения (добавлено в другой список, файл прочитан или записан).",
   "Nothing to undo": "Нечего отменять",
   "Notices": "Уведомления",
   "Off": {
    "in": [
     [
      ".ui-sw",
      "Выкл"
     ],
     [
      "#outAudience, #outStage",
      "Выкл"
     ]
    ],
    "else": "Выкл"
   },
   "ON": "ВКЛ",
   "ON SCREEN": "НА ЭКРАНЕ",
   "Open": "Открыть",
   "Open a group of choices; go back": "Открыть группу пунктов; назад",
   "Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive": "Открыть Таймеры, Сообщения, Накладки, Макросы, Сцену, Аудио, VoxLive",
   "Or use the sample backgrounds": "Или возьмите образцы фонов",
   "Order": "Порядок",
   "Password": "Пароль",
   "Paste": "Вставить",
   "Paste Style": "Вставить стиль",
   "Pause": "Пауза",
   "Pick several in the left column: grow the pick, or pick them all": "Выбрать несколько в левой колонке: расширить выбор или выбрать всё",
   "Picture": "Картинка",
   "Placeholder": "Пустое место",
   "Play": "Воспроизвести",
   "Play on Announcements Layer": "Показать на слое объявлений",
   "Play the tracks in a random order": "Воспроизводить треки в случайном порядке",
   "Play through": "Подряд",
   "Playing": "ИГРАЕТ",
   "Playlist": "Список",
   "Playlists": "Списки",
   "Playlists and libraries": "Списки и библиотеки",
   "Press / to search from anywhere": "Нажмите /, чтобы искать откуда угодно",
   "Prev": "Назад",
   "Previous": "Назад",
   "Previous slide": "Предыдущий слайд",
   "Props": "Накладки",
   "Push": "Выталкивание",
   "Ready": "ГОТОВО",
   "Reconnecting…": "Переподключение…",
   "Redo": "Повторить",
   "Remote Control": "Удалённое управление",
   "Remove from Playlist": "Убрать из списка",
   "Rename": "Переименовать",
   "Reset": "Сбросить",
   "Reveal": "Открытие",
   "Running the slides": "Показ слайдов",
   "s": "с",
   "Scaling": "Масштаб",
   "Service": "Служение",
   "Service (⌥S)": "Служение (⌥S)",
   "Service, Bible (ready for a reference), Edit (in the app ⌘1 ⌘2 ⌘3; a browser keeps those)": "Служение, Библия (готова к вводу ссылки), Редактор (в приложении ⌘1 ⌘2 ⌘3; в браузере они заняты)",
   "Settings": "Настройки",
   "Settings are changed at the computer.": "Настройки меняются за компьютером.",
   "Show": "Показать",
   "Show a message": "Показать сообщение",
   "Show keys on buttons": "Клавиши на кнопках",
   "Show that picture or video": "Показать эту картинку или видео",
   "Shuffle": "Вперемешку",
   "Sign In": "Войти",
   "Sign Out of This Device…": "Выйти на этом устройстве…",
   "Signing in…": "Вход…",
   "Slide": "Слайд",
   "Slide size": "Размер слайдов",
   "Slides…": "Слайды…",
   "SoftPresenter Remote Control": "Пульт SoftPresenter",
   "Something went wrong: click to see": "Что-то пошло не так: нажмите, чтобы посмотреть",
   "Song": "Песня",
   "Song Files…": "Файлы песен…",
   "Song libraries": "Библиотеки песен",
   "Split": "Разделение",
   "Stage": "Сцена",
   "Stage screens on or off (⇧⌘2)": "Экраны для сцены: включить или выключить (⇧⌘2)",
   "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track": "Запустить этот таймер, показать это сообщение или накладку, выполнить этот макрос, переключить сцену на этот макет, включить этот трек",
   "Swap": "Обмен",
   "Table": "Таблица",
   "Template": "Шаблон",
   "The car with the lights on, please": "Чья машина с включёнными фарами?",
   "The computer didn't answer.": "Компьютер не ответил.",
   "The cued slide or verse goes on screen (after a search, or ↑ ↓)": "Выбранный слайд или стих выходит на экран (после поиска или ↑ ↓)",
   "The item above or below, cued at its first slide; in Bible, the verse before or after": "Пункт выше или ниже; в Библии — стих до или после",
   "The keys listed in Settings › Keys do what it says there. Any other key can be added.": "Клавиши из списка Настройки › Клавиши делают то, что там указано. Можно добавить любую другую клавишу.",
   "The live tools": "Инструменты показа",
   "The menu for the slide on screen": "Меню слайда, который на экране",
   "then 1–9": "затем 1 … 9",
   "This device is remembered once it signs in, until it is forgotten in Settings › Phone remote on the computer.": "Это устройство запоминается после входа, пока его не забудут на компьютере: Настройки › Пульт на телефоне.",
   "This library has no songs yet.": "В этой библиотеке пока нет песен.",
   "This list": "Этот список (внизу — «Клавиши на кнопках»)",
   "This sample only has": "В этом примере есть только",
   "Timers": "Таймеры",
   "track": "Трек",
   "Transition": "Переход",
   "Typing in a box: bold, italic, underlined, for the words selected (or the whole box)": "При вводе в блоке: жирный, курсив, подчёркнутый — для выбранных слов (или всего блока)",
   "Undo": "Отменить",
   "Undo, redo": "Отменить, повторить",
   "Volume": "Громкость",
   "VoxLive": "VoxLive",
   "Waiting for SoftPresenter": "Ждём SoftPresenter",
   "Waiting for SoftPresenter to answer. Try again in a moment.": "Ждём ответа SoftPresenter. Попробуйте ещё раз через минуту.",
   "When a track ends, go on to the next": "Когда трек кончится, перейти к следующему",
   "Wipe": "Шторка",
   "With nothing on screen, move the cue; Space or Page Down or Enter puts it on": "Когда на экране пусто, выбрать слайд; пробел, Page Down или Enter выводит его на экран",
   "Words": {
    "in": [
     [
      ".ui-sheet",
      "Слова"
     ]
    ],
    "else": "Слова"
   },
   "Words from Text…": "Слова из текста…",
   "Words once…": "Слова один раз…",
   "Words only": "Только слова",
   "Zoom In": "Приближение",
   "Letting a phone, iPad or Mac show its screen is decided at the computer, not from the remote page.": "Разрешить телефону, iPad или Mac показывать свой экран можно только за компьютером, а не со страницы пульта.",
   "There is no track to play. Add music at the computer.": "Нет трека для воспроизведения. Добавьте музыку за компьютером."
  },
  "patterns": [
   {
    "re": "^(.+) \\(⌥(\\d), then 1 to 9 for what is in it\\)$",
    "to": "{1t} (⌥{2}, затем 1 … 9 внутри вкладки)",
    "key": "page.tool_tab.hint"
   },
   {
    "re": "^Running SoftPresenter as (.+)$",
    "to": "Вы управляете SoftPresenter с устройства {1}",
    "key": "page.conn.as"
   },
   {
    "re": "^Notices: (\\d+) need you$",
    "to": "Уведомлений, ждущих вас: {1}",
    "key": "page.notices.many"
   },
   {
    "re": "^Counts down from (.+?)$",
    "to": "Обратный отсчёт от {1}",
    "key": "tools.timer.down_from"
   },
   {
    "re": "^(.+) (\\d+(?:\\.\\d+)?)s$",
    "to": "{1t} {2} с",
    "key": "page.transition.time"
   },
   {
    "re": "^Notices: 1 needs you$",
    "to": "Уведомления: 1 ждёт вас",
    "key": "page.notices.one"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?) s$",
    "to": "{1} с",
    "key": "common.seconds_unit"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?)s$",
    "to": "{1} с",
    "key": "common.seconds_unit"
   },
   {
    "re": "^Bible · (.+)$",
    "to": "Библия · {1}",
    "key": "page.find.bible_tag"
   },
   {
    "re": "^Add to (.+?)$",
    "to": "Добавить в «{1}»",
    "key": "left.song.add_to"
   },
   {
    "re": "^(.+?) tracks$",
    "to": "{1} трека",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Song · (.+)$",
    "to": "Песня · {1}",
    "key": "find.tag.song_group"
   },
   {
    "re": "^(.+?) items$",
    "to": "{1} пункта",
    "key": "media.items"
   },
   {
    "re": "^(.+?) songs$",
    "to": "{1} песни",
    "key": "integrations.songbooks.songs"
   },
   {
    "re": "^(.+?) track$",
    "to": "{1} трек",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Start (.+?)$",
    "to": "Запустить: {1}",
    "key": "tools.steps.timer_start"
   }
  ],
  "keys": {
   "Shift": "Shift"
  },
  "format": {
   "decimal": ",",
   "group": " ",
   "group_from": 4,
   "quote_open": "«",
   "quote_close": "»"
  }
 },
 "uk": {
  "exact": {
   "A Bible passage…": "Уривок з Біблії…",
   "A header…": "Заголовок…",
   "A Library Folder…": "Папка бібліотеки…",
   "A media playlist": "Медіасписок",
   "A picture or video": "Зображення або відео",
   "A placeholder…": "Порожнє місце…",
   "A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the order they come.": "Список — це порядок служіння: пісні, уривки з Біблії, зображення й відео в тому порядку, в якому вони йдуть.",
   "A question is waiting: click to answer it": "Чекає запитання: клацніть, щоб відповісти",
   "a service, in order": "служіння, за порядком",
   "A small tag beside each button with its key. Also in Settings › Keys.": "Маленька мітка з клавішею біля кожної кнопки. Також у розділі Налаштування › Клавіші.",
   "A Song Bundle…": "Пакет пісень…",
   "A song…": "Пісня…",
   "A–Z": "А–Я",
   "Actions": "Дії",
   "Add a Bible passage": "Додати уривок з Біблії",
   "Add a header": "Додати заголовок",
   "Add a song": "Додати пісню",
   "Add a track to this playlist": "Додайте трек до цього списку",
   "Add pictures or videos": "Додати зображення або відео",
   "Add pictures or videos to show behind the words": "Додайте зображення або відео, щоб показувати їх за словами",
   "Add Slide": "Додати слайд",
   "Add the Sample Backgrounds": "Додати зразки фонів",
   "Add to playlist": "Додати до списку",
   "Add to Playlist": "Додати до списку",
   "Adding music is done at the computer for now.": "Додавати музику поки що можна за компʼютером.",
   "Adding pictures and videos from here comes with the next step. Add them at the computer for now.": "Додавати зображення й відео звідси можна буде на наступному етапі. Поки що додавайте їх за компʼютером.",
   "Advanced": {
    "in": [
     [
      ".st-navg",
      "Додатково"
     ]
    ],
    "else": "Більше"
   },
   "All Media": "Усі медіа",
   "Announce": "Оголошення",
   "Arrangement": "Порядок виконання: співати частини пісні в іншому порядку",
   "As listed": "Як є",
   "Audience": "Зал",
   "Audience screens on or off (⇧⌘1)": "Екрани для залу: увімкнути або вимкнути (⇧⌘1)",
   "Audience screens, stage screens, on or off": "Екрани для залу, екрани для сцени: увімкнути чи вимкнути",
   "Audio": "Аудіо",
   "Audio playlist": "аудіосписок",
   "Back 15 seconds": "Назад на 15 секунд",
   "Bible": "Біблія",
   "Bible, too (Bold while editing a slide)": "Також Біблія (жирний шрифт під час редагування слайда)",
   "Blinds": "Жалюзі",
   "Cancel": "Скасувати",
   "Changing songs, playlists and settings is done at the computer for now. Running the show works from here.": "Пісні, списки й налаштування поки що змінюють за компʼютером. Керувати служінням можна звідси.",
   "Changing things": "Зміни",
   "Checkerboard": "Шахівниця",
   "Choose": "Вибрати",
   "Choose a library or playlist (⌘P)": "Вибрати список або бібліотеку (⌘P)",
   "Choose a song…": "Вибрати пісню…",
   "Choosing from the menu bar is done at the computer, not from the remote page.": "Вибирати в смузі меню можна за компʼютером, не зі сторінки пульта.",
   "Choosing the stage layouts": "Вибір макетів сцени",
   "CLEAR": "ЧИСТО",
   "Clear All": "Очистити все",
   "Clear everything": "Очистити все",
   "Clear it": "Прибрати",
   "Clear the slide, media, props, audio or messages: the layer keys under the preview": "Прибрати слайд, медіа, накладки, аудіо чи повідомлення: клавіші шарів під попереднім переглядом",
   "Clearing": "Очищення",
   "Click to show it": "Клацніть, щоб показати",
   "Clock": "Годинник",
   "Close": "Закрити",
   "comes to the remote page with the next step. Use the computer for now.": "зʼявиться на сторінці пульта на наступному етапі. Поки що користуйтеся компʼютером.",
   "Connect": "Підключити",
   "Connected": "Підключено",
   "Connecting…": "Підключення…",
   "Continuous": "Підряд",
   "Copy": "Скопіювати",
   "Copy Style": "Скопіювати стиль",
   "Copy, cut, paste the picked slides (objects in Edit)": "Скопіювати, вирізати, вставити вибрані слайди (обʼєкти в Редакторі)",
   "Could not sign in.": "Не вдалося увійти.",
   "Crossfade": "ПЕРЕХІД",
   "Cut": {
    "in": [
     [
      "#foot",
      "Одразу"
     ]
    ],
    "else": "Вирізати"
   },
   "Delete": {
    "in": [
     [
      ".ui-keys",
      "Delete"
     ]
    ],
    "else": "Видалити"
   },
   "Delete Song from Library…": "Видалити пісню з бібліотеки…",
   "Delete what is picked: what Undo brings back goes at once (playlist items, slides, a media playlist’s items); songs in a library, pictures in the bin and screens ask first": "Видалити вибране: те, що повертає «Відмінити», видаляється одразу (пункти списку, слайди, пункти медіасписку); пісні в бібліотеці, зображення в медіатеці та екрани спершу питають",
   "Delete…": "Видалити…",
   "Details": "Відомості: автор, видавець і номер CCLI пісні",
   "Disable": "Вимкнути слайд",
   "Disconnected at the computer: click to connect again": "Відключено на компʼютері: клацніть, щоб підключитися знову",
   "Dissolve": "Плавно",
   "Done": "Готово",
   "Door": "Двері",
   "Drag it into a playlist": "Перетягніть у список",
   "Drag it into a playlist or a library": "Перетягніть у список або в бібліотеку",
   "Duplicate": "Дублювати",
   "Edit": {
    "in": [
     [
      "#modes",
      "Редактор"
     ]
    ],
    "else": "Редагувати"
   },
   "Edit this slide": "Редагувати цей слайд",
   "Edit Words": "Редагувати слова",
   "Every key (press ?)": "Усі клавіші (натисніть ?)",
   "Every other transition": "Усі інші переходи і їхня тривалість",
   "every song you keep": "усі ваші пісні",
   "Fade": "Згасання",
   "Fade Black": "Згасання в чорне",
   "Fade White": "Згасання в біле",
   "Fill with Song…": "Заповнити піснею…",
   "Filter": "Фільтр",
   "Find a song, a line or a verse": "Знайти пісню, рядок або вірш",
   "Find a song, a line, or a verse": "Знайти пісню, рядок або вірш",
   "First slide of the song, or verse in Bible": "Перший слайд пісні або перший вірш у Біблії",
   "Flip": "Перевертання",
   "Fly In": "Виліт",
   "Forward 15 seconds": "Уперед на 15 секунд",
   "Go to Next": "Перейти далі",
   "Go to the slide on screen": "Перейти до слайда, що на екрані",
   "Going places": "Переходи",
   "Grid": "Сітка",
   "Group": "Частина",
   "Hide": "Сховати",
   "Hide the media strip": "Сховати смугу медіа",
   "Hot Key…": "Гаряча клавіша…",
   "How long one track fades into the next": "Скільки триває плавний перехід одного треку в наступний",
   "Import Songs": "Імпортувати пісні",
   "Import songs or slides": "Імпортувати пісні або слайди",
   "Import…": "Імпортувати…",
   "In Edit: a new text box, duplicate, pick everything": "У Редакторі: нове текстове поле, дублювати, вибрати все",
   "In Edit: nudge the picked object (Shift for one pixel)": "У Редакторі: зсунути вибраний обʼєкт (із Shift — на один піксель)",
   "Iris": "Діафрагма",
   "is empty": "— порожній список",
   "Jump to a group (C for the chorus), or a slide’s own hot key": "Перейти до частини (C — приспів) або за власною гарячою клавішею слайда",
   "Keep playing until stopped: after the last track, start again from the first (with Play through off, play this track again)": "Грати, доки не зупините: після останнього треку почати знову з першого (коли «Підряд» вимкнено, повторювати цей трек)",
   "Keys": "Клавіші",
   "Label": "Мітка",
   "Last slide of the song, or verse in Bible": "Останній слайд пісні або останній вірш у Біблії",
   "Library": "Бібліотека",
   "Longest": "Найдовші",
   "Looking up the Bible comes to the remote page with the next step. Use the Bible page at the computer for now.": "Пошук у Біблії зʼявиться на сторінці пульта на наступному етапі. Поки що користуйтеся сторінкою Біблії на компʼютері.",
   "Loop": {
    "in": [
     [
      ".lv-audio",
      "По колу"
     ]
    ],
    "else": "По колу"
   },
   "Macros": "Макроси",
   "Media": "Медіа",
   "Media Playlists": "Медіасписки",
   "Menus and windows": "Меню і вікна",
   "Messages": "Повідомлення",
   "More": "Більше",
   "Move": "Переміщення",
   "Move Down": "Перемістити нижче",
   "Move In": "Насування",
   "Move Up": "Перемістити вище",
   "Mute": "Вимкнути звук",
   "New": "Новий таймер",
   "New library": "Нова бібліотека",
   "New Media Playlist…": "Новий медіасписок…",
   "New playlist": "Новий список",
   "New song": "Нова пісня",
   "Next": "Далі",
   "Next at the end of a song goes on to the next song": "«Далі» в кінці пісні переходить до наступної пісні",
   "Next slide": "Наступний слайд",
   "Next track": "Наступний трек",
   "No song is open": "Жодну пісню не відкрито",
   "No songs here yet. Make one, import some, or drag songs onto this library’s name.": "Тут ще немає пісень. Створіть пісню, імпортуйте кілька або перетягніть пісні на назву цієї бібліотеки.",
   "No tracks yet. Add music files from this computer.": "Треків ще немає. Додайте музичні файли з цього компʼютера.",
   "Not connected to SoftPresenter.": "Немає звʼязку з SoftPresenter.",
   "Notes": "Нотатки",
   "Nothing found": "Нічого не знайдено",
   "Nothing showing": "Нічого не показується",
   "Nothing to play": "НЕМАЄ ЩО ГРАТИ",
   "Nothing to redo": "Нічого повторювати",
   "Nothing to tell you. Trouble, questions, and what happens out of sight (added to another playlist, a file read or written) come here.": "Поки нічого. Сюди приходять проблеми, запитання і те, що стається поза увагою (додано до іншого списку, файл прочитано чи записано).",
   "Nothing to undo": "Нічого відміняти",
   "Notices": "Сповіщення",
   "Off": {
    "in": [
     [
      ".ui-sw",
      "Вимк"
     ],
     [
      "#outAudience, #outStage",
      "Вимк"
     ]
    ],
    "else": "Вимк"
   },
   "ON": "УВІМК",
   "ON SCREEN": "НА ЕКРАНІ",
   "Open": "Відкрити",
   "Open a group of choices; go back": "Відкрити набір варіантів; повернутися",
   "Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive": "Відкрити Таймери, Повідомлення, Накладки, Макроси, Сцену, Аудіо, VoxLive",
   "Or use the sample backgrounds": "Або візьміть зразки фонів",
   "Order": "Порядок",
   "Password": "Пароль",
   "Paste": "Вставити",
   "Paste Style": "Вставити стиль",
   "Pause": "Пауза",
   "Pick several in the left column: grow the pick, or pick them all": "Вибрати кілька в лівій колонці: розширити вибір або вибрати все",
   "Picture": "Зображення",
   "Placeholder": "Порожнє місце",
   "Play": "Відтворити",
   "Play on Announcements Layer": "Відтворити в шарі оголошень",
   "Play the tracks in a random order": "Відтворювати треки у випадковому порядку",
   "Play through": "Підряд",
   "Playing": "ГРАЄ",
   "Playlist": "Список",
   "Playlists": "Списки",
   "Playlists and libraries": "Списки й бібліотеки",
   "Press / to search from anywhere": "Натисніть /, щоб шукати будь-звідки",
   "Prev": "Назад",
   "Previous": "Назад",
   "Previous slide": "Попередній слайд",
   "Props": "Накладки",
   "Push": "Виштовхування",
   "Ready": "ГОТОВО",
   "Reconnecting…": "Перепідключення…",
   "Redo": "Повторити",
   "Remote Control": "Дистанційне керування",
   "Remove from Playlist": "Вилучити зі списку",
   "Rename": "Перейменувати",
   "Reset": "Скинути",
   "Reveal": "Відкриття",
   "Running the slides": "Ведення слайдів",
   "s": "с",
   "Scaling": "Масштаб",
   "Service": "Служіння",
   "Service (⌥S)": "Служіння (⌥S)",
   "Service, Bible (ready for a reference), Edit (in the app ⌘1 ⌘2 ⌘3; a browser keeps those)": "Служіння, Біблія (готова до введення посилання), Редактор (у програмі ⌘1 ⌘2 ⌘3; у браузері вони зайняті)",
   "Settings": "Налаштування",
   "Settings are changed at the computer.": "Налаштування змінюють за компʼютером.",
   "Show": "Показати",
   "Show a message": "Показати повідомлення",
   "Show keys on buttons": "Клавіші на кнопках",
   "Show that picture or video": "Показати це зображення чи відео",
   "Shuffle": "Вперемішку",
   "Sign In": "Увійти",
   "Sign Out of This Device…": "Вийти на цьому пристрої…",
   "Signing in…": "Вхід…",
   "Slide": "Слайд",
   "Slide size": "Розмір слайдів",
   "Slides…": "Слайди…",
   "SoftPresenter Remote Control": "Пульт SoftPresenter",
   "Something went wrong: click to see": "Щось пішло не так: клацніть, щоб побачити",
   "Song": "Пісня",
   "Song Files…": "Файли пісень…",
   "Song libraries": "Бібліотеки пісень",
   "Split": "Розділення",
   "Stage": "Сцена",
   "Stage screens on or off (⇧⌘2)": "Екрани для сцени: увімкнути або вимкнути (⇧⌘2)",
   "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track": "Запустити цей таймер, показати це повідомлення чи накладку, виконати цей макрос, перемкнути сцену на цей макет, відтворити цей трек",
   "Swap": "Обмін",
   "Table": "Таблиця",
   "Template": "Шаблон",
   "The car with the lights on, please": "Чиє авто з увімкненими фарами?",
   "The computer didn't answer.": "Компʼютер не відповів.",
   "The cued slide or verse goes on screen (after a search, or ↑ ↓)": "Вибраний слайд або вірш виходить на екран (після пошуку або ↑ ↓)",
   "The item above or below, cued at its first slide; in Bible, the verse before or after": "Пункт вище чи нижче; у Біблії — вибрати попередній чи наступний вірш",
   "The keys listed in Settings › Keys do what it says there. Any other key can be added.": "Клавіші зі списку Налаштування › Клавіші роблять те, що там указано. Можна додати будь-яку іншу клавішу.",
   "The live tools": "Під час служіння",
   "The menu for the slide on screen": "Меню слайда, що на екрані",
   "then 1–9": "потім 1 … 9",
   "This device is remembered once it signs in, until it is forgotten in Settings › Phone remote on the computer.": "Цей пристрій запамʼятовується після входу, доки його не забудуть на компʼютері: Налаштування › Пульт на телефоні.",
   "This library has no songs yet.": "У цій бібліотеці поки що немає пісень.",
   "This list": "Цей перелік (унизу є «Клавіші на кнопках»)",
   "This sample only has": "У цьому зразку є лише",
   "Timers": "Таймери",
   "track": "Трек",
   "Transition": "Перехід",
   "Typing in a box: bold, italic, underlined, for the words selected (or the whole box)": "Під час введення в полі: жирний, курсив, підкреслений — для вибраних слів (або всього поля)",
   "Undo": "Відмінити",
   "Undo, redo": "Відмінити, повторити",
   "Volume": "Гучність",
   "VoxLive": "VoxLive",
   "Waiting for SoftPresenter": "Чекаємо на SoftPresenter",
   "Waiting for SoftPresenter to answer. Try again in a moment.": "Чекаємо на відповідь SoftPresenter. Спробуйте ще раз за мить.",
   "When a track ends, go on to the next": "Коли трек закінчується, переходити до наступного",
   "Wipe": "Шторка",
   "With nothing on screen, move the cue; Space or Page Down or Enter puts it on": "Коли на екрані порожньо, вибрати слайд; пробіл, Page Down або Enter виводить його на екран",
   "Words": {
    "in": [
     [
      ".ui-sheet",
      "Слова"
     ]
    ],
    "else": "Слова"
   },
   "Words from Text…": "Слова з тексту…",
   "Words once…": "Швидкий текст…",
   "Words only": "Лише слова",
   "Zoom In": "Наближення",
   "Letting a phone, iPad or Mac show its screen is decided at the computer, not from the remote page.": "Дозволити телефону, iPad чи Mac показувати свій екран можна лише за компʼютером, не зі сторінки пульта.",
   "There is no track to play. Add music at the computer.": "Немає треку для відтворення. Додайте музику за компʼютером."
  },
  "patterns": [
   {
    "re": "^(.+) \\(⌥(\\d), then 1 to 9 for what is in it\\)$",
    "to": "{1t} (⌥{2}, потім 1 … 9 усередині вкладки)",
    "key": "page.tool_tab.hint"
   },
   {
    "re": "^Running SoftPresenter as (.+)$",
    "to": "Ви керуєте SoftPresenter із пристрою {1}",
    "key": "page.conn.as"
   },
   {
    "re": "^Notices: (\\d+) need you$",
    "to": "Сповіщень, що чекають на вас: {1}",
    "key": "page.notices.many"
   },
   {
    "re": "^Counts down from (.+?)$",
    "to": "Зворотний відлік від {1}",
    "key": "tools.timer.down_from"
   },
   {
    "re": "^(.+) (\\d+(?:\\.\\d+)?)s$",
    "to": "{1t} {2} с",
    "key": "page.transition.time"
   },
   {
    "re": "^Notices: 1 needs you$",
    "to": "Сповіщення: 1 чекає на вас",
    "key": "page.notices.one"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?) s$",
    "to": "{1} с",
    "key": "common.seconds_unit"
   },
   {
    "re": "^(\\d+(?:\\.\\d+)?)s$",
    "to": "{1} с",
    "key": "common.seconds_unit"
   },
   {
    "re": "^Bible · (.+)$",
    "to": "Біблія · {1}",
    "key": "page.find.bible_tag"
   },
   {
    "re": "^Add to (.+?)$",
    "to": "Додати до «{1}»",
    "key": "left.song.add_to"
   },
   {
    "re": "^(.+?) tracks$",
    "to": "{1} треки",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Song · (.+)$",
    "to": "Пісня · {1}",
    "key": "find.tag.song_group"
   },
   {
    "re": "^(.+?) items$",
    "to": "{1} пункти",
    "key": "media.items"
   },
   {
    "re": "^(.+?) songs$",
    "to": "{1} пісні",
    "key": "integrations.songbooks.songs"
   },
   {
    "re": "^(.+?) track$",
    "to": "{1} трек",
    "key": "tools.audio.tracks"
   },
   {
    "re": "^Start (.+?)$",
    "to": "Запустити {1}",
    "key": "tools.steps.timer_start"
   }
  ],
  "keys": {
   "Shift": "Shift"
  },
  "format": {
   "decimal": ",",
   "group": " ",
   "group_from": 4,
   "quote_open": "«",
   "quote_close": "»"
  }
 }
};
'use strict';
// The remote page in the app's language (designed 24 November). The page's words are written in English in the code; this
// puts them in the language the app is set to, from a table built out of the app's own tables (translation-review/es:
// build-remote-table.js → remote-words.js). The page never chooses: the app's choice comes in the served page
// (window.SP_LANGUAGE) and then live in the status (parts.app), so a team reads the same words as the operator.
//
// Content is never touched: song words and titles, names, files, verses, keys, anything typed or anything the app sends.
// Each word keeps its English, so switching back, or to another language, starts from the English every time.
// With English, or a language the table doesn't have, nothing changes at all.

const UI_LANGS = ['en', 'es', 'ru', 'uk'];
// What is content: langtool's list (less the keys: a key's name is a word of the page's, "Space" is Espacio, Shift is
// Mayús), and the remote page's own (a message's words, a header's name, a group's name on a
// slide, the running timer's name and time). The harvest of the page's words reads this same line.
const UI_CONTENT = '.thumb, #canvas, .canvas, .ob, .sl-words, .tt, .clip .nm, .grp-h b, .verse, .vt, input, textarea, select option[value=""] ~ option, .st-key, .readout .txtline, #found .t, .ed-songs .t, .plbtn > span, .sh-src .t, .mono.num, .item .num, script, style, svg, .cat-words, .bi-hit, .lv-name, .lv-tname, .lv-track .t, [data-c="audioPl"] option, .card .t b, .card .t span, .sh-hrow .hn, .sh-head, .strip .lbl:not(.say), #found .fd-row b, #found .fd-row em, #runTimer, #remotetest, #selftest, .rm-content';
const UI_ATTRS = ['title', 'placeholder', 'aria-label'];

const UI = (() => {
  let lang = 'en', words = null, format = null, titleSrc = null;
  const srcText = new WeakMap(), outText = new WeakMap();      // a text node → its English, and what was last written in it
  const srcAttr = new WeakMap();                               // an element → { attr: { src, out } }
  const isContent = el => !!(el && el.closest && el.closest(UI_CONTENT));
  // A number as the language writes it: "13,613" → "13.613" (grouped from five figures), "0.4" → "0,4"
  const num = s => { if (!format) return s; const m = /^(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d+))?$/.exec(s); if (!m) return s;
    const whole = m[1].replace(/,/g, ''), grouped = whole.length >= (format.group_from || 4) ? whole.replace(/\B(?=(\d{3})+(?!\d))/g, format.group || ',') : whole;
    return grouped + (m[2] != null ? (format.decimal || '.') + m[2] : ''); };
  // One English word, two meanings: the word where it stands ("Cut" in the bar under the slides is Corte, in a menu Cortar)
  const inPlace = (w, el) => { for (const [sel, es] of w.in) if (el && el.closest && el.closest(sel)) return es; return w.else; };
  const say = (en, el) => {
    const t = en.replace(/\s+/g, ' ').trim(); if (!t || !words) return en;
    const lead = en.match(/^\s*/)[0], tail = en.match(/\s*$/)[0];
    if (Object.prototype.hasOwnProperty.call(words.exact, t)) { const w = words.exact[t]; return lead + (typeof w === 'string' ? w : inPlace(w, el)) + tail; }
    // A sentence with values: each value as the language writes numbers, or ("{1t}") a word of the page's own put in the language too
    if (el && el.closest && el.closest('kbd, .kh') && words.keys) { const k = t.replace(/\b(Shift|Ctrl|Alt)\b/g, w => words.keys[w] || w); if (k !== t) return lead + k + tail; }      // "Ctrl+Shift+Z" → "Ctrl+Mayús+Z"
    for (const p of words.patterns) { const m = p.re.exec(t); if (m) return lead + p.to.replace(/\{(\d+)(t?)\}/g, (_, i, tr) => tr ? say(m[+i], el).trim() : num(m[+i])) + tail; }
    return en;
  };
  function textNode(n) {
    if (isContent(n.parentElement)) return;
    const now = n.nodeValue, src = outText.get(n) === now ? srcText.get(n) : now;      // written by someone else since: that is the new English
    srcText.set(n, src); const out = lang === 'en' ? src : say(src, n.parentElement);
    outText.set(n, out); if (out !== now) n.nodeValue = out;
  }
  function attrs(el) {
    if (!el.getAttribute) return;
    const field = el.matches && el.matches('input, textarea');
    if (isContent(el) && !field) return;
    let rec = srcAttr.get(el); if (!rec) srcAttr.set(el, rec = {});
    for (const a of UI_ATTRS) {
      if (field && a !== 'placeholder' && a !== 'aria-label' && a !== 'title') continue;
      const now = el.getAttribute(a); if (now == null) continue;
      const r = rec[a], src = r && r.out === now ? r.src : now, out = lang === 'en' ? src : say(src, el);
      rec[a] = { src, out }; if (out !== now) el.setAttribute(a, out);
    }
  }
  function pass(root) {
    if (!root) return;
    if (root.nodeType === 3) return textNode(root);
    if (root.nodeType !== 1 && root.nodeType !== 9) return;
    const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) textNode(n);
    if (root.nodeType === 1) attrs(root);
    for (const el of root.querySelectorAll ? root.querySelectorAll('*') : []) attrs(el);
  }
  let watching = false;
  function watch() {
    if (watching || typeof MutationObserver !== 'function') return; watching = true;
    new MutationObserver(ms => { if (lang === 'en' && !words) return;
      for (const m of ms) {
        if (m.type === 'childList') m.addedNodes.forEach(pass);
        else if (m.type === 'characterData') textNode(m.target);
        else if (m.type === 'attributes') attrs(m.target);
      } }).observe(document, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: UI_ATTRS });
  }
  // The language the app is set to, and its number format. Anything but the four codes is English.
  function set(code, fmt) {
    const next = UI_LANGS.includes(code) ? code : 'en';
    const table = next !== 'en' && typeof REMOTE_WORDS === 'object' && REMOTE_WORDS[next] ? REMOTE_WORDS[next] : null;
    format = fmt && typeof fmt === 'object' ? { decimal: String(fmt.decimal || '.').slice(0, 2), group: String(fmt.group == null ? ',' : fmt.group).slice(0, 2), group_from: Number(fmt.group_from) || 4 }
      : table && table.format ? table.format : null;
    const was = lang; lang = table ? next : 'en';
    words = table ? { exact: table.exact, keys: table.keys || null, patterns: (table.patterns || []).map(p => ({ re: new RegExp(p.re), to: p.to })) } : null;
    document.documentElement.lang = table ? next : 'en';
    if (lang !== 'en' || was !== 'en') {
      watch(); pass(document.body);
      if (titleSrc == null) titleSrc = document.title;
      document.title = lang === 'en' ? titleSrc : say(titleSrc);
    }
    return lang;
  }
  return { set, get lang() { return lang; }, say: en => (lang === 'en' ? en : say(en)) };
})();
// The served page says the app's language before anything else; in the prototype, ?uilang= stands in for it
UI.set(window.SP_LANGUAGE || new URLSearchParams(location.search).get('uilang') || 'en');
(function () {
  const form = document.getElementById("signin");
  const field = document.getElementById("password");
  const said = document.getElementById("said");
  // The button waits for this script, so pressing Enter early can never send the password some other way.
  document.getElementById("signinGo").disabled = false;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    said.textContent = "Signing in…";
    try {
      const answer = await fetch("/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password: field.value }),
        credentials: "same-origin",
      });
      const reply = await answer.json().catch(() => ({}));
      if (answer.ok && reply.ok) {
        field.value = "";
        location.replace("/");
        return;
      }
      said.textContent = reply.text || "Could not sign in.";
    } catch (problem) {
      said.textContent = "The computer didn't answer.";
    }
  });
})();
