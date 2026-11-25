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
   "Adding music is done at the computer for now.": "Por ahora, la música se añade en el ordenador.",
   "Adding pictures and videos from here comes with the next step. Add them at the computer for now.": "Añadir imágenes y vídeos desde aquí llegará en el próximo paso. Por ahora, añádelos en el ordenador.",
   "Advanced": "Más",
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
   "Cancel": "Cancelar",
   "Changing songs, playlists and settings is done at the computer for now. Running the show works from here.": "Por ahora, las canciones, las listas y los ajustes se cambian en el ordenador. Llevar la presentación sí se puede desde aquí.",
   "Changing things": "Cambios",
   "Choose": "Elegir",
   "Choose a library or playlist (⌘P)": "Elegir una lista o una biblioteca (⌘P)",
   "Choose a song…": "Elegir una canción…",
   "Choosing from the menu bar is done at the computer, not from the remote page.": "Lo que está en la barra de menús se elige en el ordenador, no desde esta página.",
   "Choosing the stage layouts": "Elegir los diseños del escenario",
   "CLEAR": "DESPEJADA",
   "Clear All": "Borrar todo",
   "Clear everything": "Borrar todo",
   "Clear it": "Quitarlo",
   "Clear the slide, media, props, audio, message": "Quitar la diapositiva, lo multimedia, los gráficos, el audio, el mensaje",
   "Clearing": "Borrar",
   "Click to show it": "Haz clic para mostrarlo",
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
   "Fill with Song…": "Rellenar con una canción…",
   "Filter": "Filtrar",
   "Find a song, a line or a verse": "Buscar una canción, una línea o un versículo",
   "Find a song, a line, or a verse": "Buscar canción, línea o versículo",
   "First or last slide of the song": "Primera o última diapositiva de la canción",
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
   "is empty": "está vacía",
   "Jump to a group (C for the chorus), or a slide’s own hot key": "Saltar a un grupo (C para el coro), o a la tecla propia de una diapositiva",
   "Keep playing until stopped: after the last track, start again from the first (with Play through off, play this track again)": "Seguir sonando hasta que lo pares: tras la última pista, volver a empezar por la primera (con Continuo apagado, repetir esta pista)",
   "Keys": "Teclas",
   "Label": "Etiqueta",
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
   "Ready": "Listo",
   "Reconnecting…": "Reconectando…",
   "Redo": "Rehacer",
   "Remote Control": "Control remoto",
   "Remove from Playlist": "Quitar de la lista",
   "Rename": "Renombrar",
   "Reset": "Reiniciar",
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
   "Stage": "Escenario",
   "Stage screens on or off (⇧⌘2)": "Pantallas del escenario: encender o apagar (⇧⌘2)",
   "Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track": "Iniciar ese temporizador, mostrar ese mensaje o gráfico, ejecutar esa macro, poner el escenario en ese diseño, reproducir esa pista",
   "Table": "Tabla",
   "Template": "Plantilla",
   "The car with the lights on, please": "El coche con las luces encendidas, por favor",
   "The computer didn't answer.": "El ordenador no respondió.",
   "The cued slide or verse goes on screen (after a search, or ↑ ↓)": "La diapositiva o el versículo marcado pasa a pantalla (después de una búsqueda, o con ↑ ↓)",
   "The first nine can be changed in Settings › Keys.": "Las nueve primeras se pueden cambiar en Ajustes › Teclas.",
   "The item above or below, cued at its first slide; in Bible, the verse before or after": "El elemento de arriba o de abajo; en Biblia, elegir el versículo anterior o siguiente",
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
   "With nothing on screen, move the cue; Space or Enter puts it on": "Sin nada en pantalla, mueve la marca; Espacio o Intro la pone en pantalla",
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
