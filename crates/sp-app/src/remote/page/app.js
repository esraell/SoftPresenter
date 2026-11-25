'use strict';
// SoftPresenter Remote Control: the page, packed by bundle.py from the design prototype. Every word from the app goes in as text (dom.js).
window.SP_REMOTE = true;

/* ═════════ dom.js ═════════ */
'use strict';
// The page's one way to draw.
//
// html`…` keeps the page's own markup (the fixed text of the template, written here) apart from every value put
// into it. A value is only ever words (a text node) or part of an attribute's value (setAttribute); it is never read
// as markup, so a song title, a slide's words or a notice can't become an element, an attribute or a script,
// whatever they hold. There is no escaping step to get wrong, and no way to hand html a finished string: it takes
// template literals only.
//
// The fixed text of each template is read once by the small reader below and kept. Nothing here hands a string to
// the browser to read as markup, so the page runs under the remote server's policy (Trusted Types 'none', no inline
// script or style).
//
//   put(el, html`<b class="${cls}">${title}</b>`, more…)   replaces el's children
//   html`<button ${{ disabled: !can, title: why }}>…`       an object in a tag sets attributes (false or null: left out)
//   values may be words, numbers, html`…`, arrays of them, DOM nodes, or null / false / '' (nothing)

const { html, put, isHtml, seams } = (() => {
const HTML = Symbol('html');
const SVG_NS = 'http://www.w3.org/2000/svg';
const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr']);
const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', thinsp: ' ', times: '×', middot: '·', hellip: '…',
  rarr: '→', larr: '←', uarr: '↑', darr: '↓', mdash: '—', ndash: '–', bull: '•', copy: '©', minus: '−', zwnj: '‌', shy: '­' };
const NAME = /^[a-zA-Z][a-zA-Z0-9-]*$/, ATTR = /^[a-zA-Z_:][-a-zA-Z0-9_:.]*$/;
const NEVER = /^(on|srcdoc$|formaction$|action$)/i;                       // no attribute that runs or posts anything
const URL_ATTR = /^(src|href|xlink:href|poster)$/i;
// A picture or link may point only at this page's own files, a blob this page made, or a #fragment: no backslash, space or
// control character (a browser reads "/\\host" as another computer), and the same origin once resolved.
const sameOrigin = v => { if (v[0] === '#') return true; if (/[\\\u0000- \u007f]/.test(v)) return false;
  try { const u = new URL(v, location.href); return u.protocol === 'blob:' ? new URL(u.pathname).origin === location.origin : u.origin === location.origin; } catch (_) { return false; } };

function html(strings, ...values) {
  if (!Array.isArray(strings) || !Array.isArray(strings.raw) || !Object.isFrozen(strings)) throw new TypeError('html`…` takes a template literal, never a string');
  return { [HTML]: true, strings, values, toString: asText };
}
const isHtml = x => !!x && x[HTML] === true;
// A piece is never turned into a string: that would hand markup around as words. Anything that tries says so at once.
function asText() { throw new TypeError('html: a piece was turned into a string; give it to put() or to another html`…` instead'); }

/* ───────── reading a template's fixed text, once ───────── */

const READ = new WeakMap();
function decode(s) {
  return s.indexOf('&') < 0 ? s : s.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (m, e) => {
    if (e[0] === '#') return String.fromCodePoint(e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10));
    if (!(e in ENTITIES)) throw new SyntaxError(`html: unknown entity ${m}`);
    return ENTITIES[e];
  });
}
function read(strings) {
  let tree = READ.get(strings);
  if (tree) return tree;
  const root = { children: [] }, stack = [root], where = () => strings.join('${…}').slice(0, 160);
  let mode = 'text', buf = '', el = null, attr = null, quote = '';
  const top = () => stack[stack.length - 1];
  const words = () => { if (buf) top().children.push({ text: decode(buf) }); buf = ''; };
  const open = selfClosing => {
    if (!NAME.test(el.tag)) throw new SyntaxError(`html: bad tag <${el.tag}> in ${where()}`);
    top().children.push(el);
    if (!selfClosing && !VOID.has(el.tag.toLowerCase())) stack.push(el);
    el = null; mode = 'text';
  };
  const close = name => {
    for (let k = stack.length - 1; k > 0; k--) if (stack[k].tag.toLowerCase() === name.toLowerCase()) { stack.length = k; return; }
  };
  const named = name => {
    if (!ATTR.test(name) || NEVER.test(name)) throw new SyntaxError(`html: attribute “${name}” is not allowed, in ${where()}`);
    return name;
  };
  for (let i = 0; i < strings.length; i++) {
    const s = strings[i];
    for (let j = 0; j < s.length; j++) {
      const c = s[j];
      if (mode === 'text') {
        if (c !== '<') { buf += c; continue; }
        words();
        if (s.startsWith('<!--', j)) { const e = s.indexOf('-->', j); if (e < 0) throw new SyntaxError(`html: a comment must end in the same piece, in ${where()}`); j = e + 2; continue; }
        if (s[j + 1] === '/') { const e = s.indexOf('>', j); if (e < 0) throw new SyntaxError(`html: a closing tag must end in the same piece, in ${where()}`); close(s.slice(j + 2, e).trim()); j = e; continue; }
        mode = 'tagname'; continue;
      }
      if (mode === 'tagname') {
        if (/[\s/>]/.test(c)) { el = { tag: buf, attrs: [], children: [] }; buf = ''; mode = 'tag'; j--; continue; }
        buf += c; continue;
      }
      if (mode === 'tag') {
        if (/\s/.test(c)) continue;
        if (c === '>') { open(false); continue; }
        if (c === '/' && s[j + 1] === '>') { j++; open(true); continue; }
        mode = 'attrname'; buf = c; continue;
      }
      if (mode === 'attrname') {
        if (c === '=') { attr = { name: named(buf), parts: [] }; buf = ''; mode = 'beforevalue'; continue; }
        if (/[\s/>]/.test(c)) { el.attrs.push({ name: named(buf), parts: [] }); buf = ''; mode = 'tag'; j--; continue; }
        buf += c; continue;
      }
      if (mode === 'beforevalue') {
        if (c === '"' || c === "'") { quote = c; mode = 'value'; continue; }
        quote = ''; mode = 'value'; j--; continue;
      }
      if (mode === 'value') {
        if (quote ? c === quote : /[\s>]/.test(c)) {
          if (buf) attr.parts.push(decode(buf));
          buf = ''; el.attrs.push(attr); attr = null; mode = 'tag'; if (!quote) j--; continue;
        }
        buf += c; continue;
      }
    }
    if (i === strings.length - 1) break;
    // a value goes here: as words, inside a quoted attribute value, or (an object) among a tag's attributes
    if (mode === 'text') { words(); top().children.push({ slot: i }); }
    else if (mode === 'value' && quote) { if (buf) attr.parts.push(decode(buf)); buf = ''; attr.parts.push({ slot: i }); }
    else if (mode === 'tag') el.attrs.push({ spread: i });
    else throw new SyntaxError(`html: a value can’t go here (${mode === 'value' ? 'an attribute value needs quotes' : mode}), in ${where()}`);
  }
  if (mode !== 'text') throw new SyntaxError(`html: the template ends inside a tag, in ${where()}`);
  words();
  READ.set(strings, root);
  return root;
}

/* ───────── making the nodes ───────── */

function setAttr(el, name, value) {
  if (!ATTR.test(name) || NEVER.test(name)) throw new TypeError(`html: attribute “${name}” is not allowed`);
  if (name === 'style') { el.style.cssText = value; return; }           // through the style object, which the page's policy allows
  if (URL_ATTR.test(name) && value !== '' && !sameOrigin(value)) throw new TypeError(`html: “${name}” may only point at this page’s own files or a blob`);
  el.setAttribute(name, value);
}
function attrPart(v) {
  if (isHtml(v) || Array.isArray(v) || (v && typeof v === 'object')) throw new TypeError('html: only words and numbers go inside an attribute');
  return String(v);
}
function make(n, values, ns, out) {
  if (n.text !== undefined) { out.push(document.createTextNode(n.text)); return; }
  if (n.slot !== undefined) { nodes(values[n.slot], ns, out); return; }
  const myNs = n.tag === 'svg' ? SVG_NS : ns;
  const el = myNs ? document.createElementNS(myNs, n.tag) : document.createElement(n.tag);
  for (const a of n.attrs) {
    if (a.spread !== undefined) {
      const o = values[a.spread];
      if (o == null || o === false || o === '') continue;
      if (typeof o !== 'object' || Array.isArray(o) || isHtml(o)) throw new TypeError('html: only an object of attributes goes inside a tag');
      for (const k of Object.keys(o)) { const v = o[k]; if (v == null || v === false) continue; setAttr(el, k, v === true ? '' : attrPart(v)); }
      continue;
    }
    setAttr(el, a.name, a.parts.map(p => typeof p === 'string' ? p : attrPart(values[p.slot])).join(''));
  }
  const kids = [], kidNs = myNs === SVG_NS && n.tag !== 'foreignObject' ? SVG_NS : null;
  for (const c of n.children) make(c, values, kidNs, kids);
  if (kids.length) el.append(...kids);
  out.push(el);
}
function nodes(x, ns = null, out = []) {
  if (x == null || x === false || x === true || x === '') return out;
  if (typeof x === 'string' || typeof x === 'number') { out.push(document.createTextNode(String(x))); return out; }
  if (Array.isArray(x)) { for (const y of x) nodes(y, ns, out); return out; }
  if (isHtml(x)) { for (const n of read(x.strings).children) make(n, x.values, ns, out); return out; }
  if (x instanceof Node) { out.push(x); return out; }
  throw new TypeError('html: this can’t be drawn: ' + Object.prototype.toString.call(x));
}
// Replaces el's children with what is given.
function put(el, ...content) {
  if (typeof el === 'string') el = document.getElementById(el);
  el.replaceChildren(...nodes(content, el.namespaceURI === SVG_NS ? SVG_NS : null));
  if (typeof LANG_PASS === 'function') LANG_PASS(el);      // design aid only (langtool.js, never shipped): the words in another language, or stretched
  return el;
}

/* ───────── for the checks: seams a half-converted screen leaves ───────── */
// Words that look like markup, a stringified object, or an escape that was escaped twice.
function seams(root = document.body) {
  const bad = [], look = (s, where) => { const m = /<[a-z][a-z0-9]*[\s>]|\[object |&(amp|lt|gt|quot|#\d+);/i.exec(s); if (m) bad.push(`${where}: …${s.slice(Math.max(0, m.index - 30), m.index + 40)}…`); };
  const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
  for (let n = w.currentNode; n; n = w.nextNode()) {
    if (n.nodeType === 3) { if (!n.parentElement || !/^(SCRIPT|STYLE|TEXTAREA|PRE)$/.test(n.parentElement.tagName)) look(n.data, 'text'); continue; }
    for (const a of n.attributes) if (a.name !== 'style' && a.name !== 'value') look(a.value, `${n.tagName.toLowerCase()}[${a.name}]`);
  }
  return bad;
}

return { html, put, isHtml, seams };
})();

/* ═════════ data.js ═════════ */
// Sample data for the prototype. DOC is everything the app would save; nothing here is real church data.
'use strict';

let uid = 0;
const newId = p => (p || 'x') + (++uid);
const clone = x => JSON.parse(JSON.stringify(x));

// The slide face built into the app (SoftPresenter Sans, OFL: fonts/OFL.txt) comes first, as in the app's font list; then the computer's own fonts.
const DEF_FONT = 'SoftPresenter Sans';
const FONTS = { 'SoftPresenter Sans': "'SoftPresenter Sans', 'Helvetica Neue', Helvetica, Arial, sans-serif",
  Helvetica: "'Helvetica Neue', Helvetica, Arial, sans-serif", Arial: 'Arial, sans-serif', 'Avenir Next': "'Avenir Next', Avenir, 'Segoe UI', sans-serif", Futura: "Futura, 'Century Gothic', sans-serif", 'Gill Sans': "'Gill Sans', 'Gill Sans MT', sans-serif", Optima: 'Optima, Candara, sans-serif', Verdana: 'Verdana, sans-serif', 'Trebuchet MS': "'Trebuchet MS', sans-serif", Impact: 'Impact, sans-serif',
  Georgia: "Georgia, 'Times New Roman', serif", 'Linux Biolinum': "'Linux Biolinum', 'Linux Libertine', Georgia, serif", 'Times New Roman': "'Times New Roman', Times, serif", Palatino: "Palatino, 'Palatino Linotype', serif", Baskerville: 'Baskerville, Georgia, serif', Didot: 'Didot, Georgia, serif', 'Marker Felt': "'Marker Felt', 'Comic Sans MS', cursive", 'Courier New': "'Courier New', monospace", Menlo: 'Menlo, Consolas, monospace' };
// On a Mac the F keys belong to the system (brightness, volume, Mission Control): SoftPresenter uses none there
const MAC = /Mac|iPhone|iPad/.test((typeof navigator !== 'undefined' && (navigator.platform || navigator.userAgent)) || '');
const SWATCHES = ['#ffffff', '#000000', '#d2cec6', '#ff4a1a', '#ffc400', '#34c56b', '#4285f4', '#cc4eb4'];
const SHAPES = { rect: 'Rectangle', rounded: 'Rounded rectangle', ellipse: 'Ellipse', triangle: 'Triangle', rhombus: 'Rhombus', hexagon: 'Polygon', star: 'Star', arrow: 'Arrow' };
const TRANSITIONS = ['Cut', 'Dissolve', 'Fade', 'Fade Black', 'Fade White', 'Push', 'Move In', 'Reveal', 'Fly In', 'Zoom In', 'Door', 'Flip', 'Swap', 'Wipe', 'Split', 'Blinds', 'Iris', 'Clock', 'Checkerboard'];
const LABELS = ['Blank Slide', 'Instrumental', 'Repeat', 'Spoken Word'];
const LINKS = { clock: 'The clock', date: 'The date', timer: 'A timer', title: "The song's title", group: "The slide's group", label: "The slide's label", notes: "The slide's notes", next: "The next slide's words", current: 'The words on screen', stagemsg: 'The stage message', copyright: 'The copyright line' };
const DEF_STYLE = { font: DEF_FONT, size: 88, bold: true, italic: false, underline: false, strike: false, color: '#ffffff', align: 'center', valign: 'middle', stroke: false, strokeWidth: 6, shadow: true, spacing: 0, lineHeight: 1.25, caps: 'none', scaling: 'Text down', bullets: 'none', bg: null, scroll: 'none', shadowAngle: 315, shadowOffset: 10, shadowBlur: 15, shadowOpacity: .5, margin: 0 };

/* ───────── objects, slides, songs ───────── */
const obj = (kind, name, more) => ({ id: newId('o'), kind, name, opacity: 1, hidden: false, locked: false, ...more });
const words = (role, text) => obj('text', role === 'main' ? 'Main' : 'Translation', { role, text });
const box = (name, role, frame, style, text) => obj('text', name, { role, frame, style, text });
const slide = (group, pri, tr, more) => ({ id: newId('s'), group, label: '', notes: '', enabled: true, hotkey: '', advance: 0, transition: null, template: null, actions: [], media: null,
  objects: tr ? [words('main', pri), words('translation', tr)] : [words('main', pri)], ...more });
const song = (title, groups, more) => ({ id: newId('song'), title, info: { artist: '', author: '', publisher: '', year: '', ccli: '' }, theme: null, transition: null, loop: false, arrangements: [], arrangement: null,
  slides: groups.flatMap(([name, ...sl]) => sl.map(([pri, tr, extra]) => slide(name, pri, tr, extra))), ...more });

const light = song('Light Over the Hills', [
  ['Verse 1', ['Morning breaks across the bay\nAnd the hills are bright again', 'Утро встаёт над заливом\nИ холмы снова светлы'], ['Every shadow slips away\nAs the light comes in', 'Каждая тень уходит\nКогда приходит свет']],
  ['Chorus', ['Light over the hills, light over the sea\nWe sing together across the lands', 'Ми підносимо голоси й руки\nМи співаємо разом у кожнім краї']],
  ['Verse 2', ['Evening settles on the quiet sea\nAnd the lanterns glow', 'Вечер ложится на тихое море\nИ фонари горят'], ['Rest now, the day is done', '', { label: 'Instrumental' }]],
  ['Ending', ['One song, one heart', 'Одна песня, одно сердце']],
], { info: { artist: 'Sample Singers', author: 'A. Writer', publisher: 'Harbour Music', year: '2021', ccli: '1234567' } });
light.arrangements = [{ id: newId('arr'), name: 'As sung', groups: ['Verse 1', 'Chorus', 'Verse 2', 'Chorus', 'Ending'] }];
light.arrangement = light.arrangements[0].id;
const harbour = song('Harbour Lights', [
  ['Verse 1', ['When the night is long and the sea is wide\nYou are the light on the farther side', 'Когда ночь длинна и море широко\nТы свет на том берегу далеко']],
  ['Chorus', ['Bring us home, bring us home\nOver every wave we have known', 'Приведи нас домой, домой\nЧерез волны, что были с тобой']],
  ['Verse 2', ['Every storm we passed you were standing near\nEvery word of yours has kept us here', 'В каждой буре ты был рядом с нами\nТвоё слово держит нас годами']],
]);
// Two titles named by their first line, as many churches name hymns: long enough to take a second line in the list (the words are written for this sample).
const steady = song('Я построю дом на твёрдой земле', [
  ['Verse 1', ['I will build my house on steady ground\nWhere the rain may fall and the wind may sound', 'Я построю дом на твёрдой земле\nПусть идёт дождь, пусть ветер во мгле']],
  ['Chorus', ['You are my rock, you are my song\nYou are the place where I belong', 'Ты моя скала, ты песнь моя\nТы то место, где буду я']],
  ['Bridge', ['Steady, steady, steady ground', 'Твёрдая, твёрдая земля']],
]);
const evening = song('The Day Is Folding into Night', [
  ['Verse 1', ['The day is folding into night\nAnd every window holds a light', '']],
  ['Chorus', ['Keep us, keep us through the dark\nUntil the morning finds us', '']],
]);
const carol = song('Winter Carol', [
  ['Verse 1', ['Snow upon the rooftops, candles in the hall\nA quiet song is rising, a welcome for us all', '']],
  ['Chorus', ['Sing, sing the winter through\nThe longest night has light in view', '']],
]);
const notices = song('This Week', [
  ['Intro', ['Welcome!\nWe are glad you are here', '', { advance: 8 }]],
  ['Tag', ['Lunch together after the service\nEveryone is invited', '', { advance: 8 }]],
  ['Ending', ['Youth evening · Friday 7:00', '', { advance: 8 }]],
], { loop: true });

const MEDIA_BG = {
  graphite: 'radial-gradient(120% 100% at 50% 0%, #34343a 0%, #1b1b1f 45%, #0b0b0d 100%)',
  grid: 'linear-gradient(rgba(255,255,255,.06) 1px, transparent 1px) 0 0 / 100% 12.5%, linear-gradient(90deg, rgba(255,255,255,.06) 1px, transparent 1px) 0 0 / 7.03% 100%, #121214',
  ember: 'radial-gradient(90% 70% at 50% 118%, rgba(255,74,26,.55) 0%, rgba(255,74,26,.12) 45%, transparent 70%), #0c0c0e',
  drift: 'repeating-linear-gradient(115deg, rgba(255,255,255,.05) 0 2%, transparent 2% 7%), linear-gradient(180deg, #1d1d22, #0c0c0e)',
  count: 'radial-gradient(60% 60% at 50% 50%, #2a2a30 0%, #0c0c0e 70%)',
};

/* ───────── themes: a template's boxes say where each kind of words goes and how it looks ───────── */
// The built-in theme's words (the owner, 23 November): white over the second language in Stone (#d2cec6), both bold, a thin
// black outline (2 out from the letters) and a soft shadow straight down. Saved themes keep their own look.
const STONE = '#d2cec6';
// The shadow's opacity is 0.3: the app draws a shadow as six strokes of even strength, so 0.3 there looks like 0.6 in a
// browser's soft blur (the app's renders, master 0e002ec). The prototype keeps the app's number.
const WORDS_LOOK = { bold: true, stroke: true, strokeWidth: 2, shadow: true, shadowAngle: 270, shadowOffset: 4, shadowBlur: 18, shadowOpacity: .3 };
const twoLang = () => ({ id: newId('tpl'), name: 'Two languages', for: 'songs', objects: [
  box('Main', 'main', { x: 120, y: 170, w: 1680, h: 400 }, { size: 96, ...WORDS_LOOK }, 'The words of the song\nin the first language'),
  box('Translation', 'translation', { x: 120, y: 610, w: 1680, h: 330 }, { size: 64, color: STONE, ...WORDS_LOOK }, 'Слова песни\nна втором языке')] });
const oneLang = () => ({ id: newId('tpl'), name: 'One language', for: 'songs', objects: [
  box('Main', 'main', { x: 120, y: 240, w: 1680, h: 600 }, { size: 96, ...WORDS_LOOK }, 'The words of the song')] });
// The Bible template, as the owner's own app has it: the first translation's verse and reference on the top half, the second's below;
// Set in the app's slide face (as the app has it, 05b5ebe: every computer has it), white on black, verses to the left and middled,
// references bold to the right; verse type starts at 96 and comes down to fit.
const bibleTpl = () => ({ id: newId('tpl'), name: 'Bible', for: 'bible', objects: [
  box('Verse 1', 'verse1', { x: 65, y: 18, w: 1790, h: 378 }, { font: DEF_FONT, size: 96, bold: false, align: 'left', valign: 'middle', shadow: false, color: '#ffffff', lineHeight: 1.25 }, 'The first Bible’s verse'),
  box('Reference 1', 'ref1', { x: 65, y: 429, w: 1790, h: 110 }, { font: DEF_FONT, size: 44, bold: true, align: 'right', valign: 'middle', shadow: false, color: '#ffffff' }, 'От Иоанна 3:16'),
  box('Verse 2', 'verse2', { x: 65, y: 540, w: 1790, h: 378 }, { font: DEF_FONT, size: 96, bold: false, align: 'left', valign: 'middle', shadow: false, color: '#ffffff', lineHeight: 1.25 }, 'The second Bible’s verse'),
  box('Reference 2', 'ref2', { x: 65, y: 951, w: 1790, h: 110 }, { font: DEF_FONT, size: 44, bold: true, align: 'right', valign: 'middle', shadow: false, color: '#ffffff' }, 'John 3:16')] });
const lowerThird = { id: newId('theme'), name: 'Lower third', bg: '#000000', picture: null, templates: [
  { id: newId('tpl'), name: 'Words in a band', for: 'songs', objects: [
    obj('shape', 'Band', { shape: 'rect', fill: '#000000', opacity: .7, frame: { x: 0, y: 760, w: 1920, h: 320 } }),
    box('Main', 'main', { x: 120, y: 780, w: 1680, h: 170 }, { size: 58 }, 'The words of the song'),
    box('Translation', 'translation', { x: 120, y: 950, w: 1680, h: 110 }, { size: 40, color: STONE }, 'Слова песни')] },
  { id: newId('tpl'), name: 'Verse in a band', for: 'bible', objects: [
    obj('shape', 'Band', { shape: 'rect', fill: '#000000', opacity: .7, frame: { x: 0, y: 760, w: 1920, h: 320 } }),
    box('Verse 1', 'verse1', { x: 120, y: 780, w: 1680, h: 220 }, { font: 'Georgia', size: 40, bold: false, align: 'left', shadow: false }, 'The verse'),
    box('Reference 1', 'ref1', { x: 120, y: 1000, w: 1680, h: 60 }, { font: 'Georgia', size: 32, align: 'right', shadow: false }, 'John 3:16')] }] };
// A theme's versions: extra templates under a name shared across themes ("Offering"), used while that version is on.
// The sample: during the offering the room moves the words up and puts a giving band where the translation was; the stream
// keeps its band, narrower, with the code beside it. (The code is drawn as a pattern; in a real theme it is a picture.)
const VERSIONS = [{ id: 'ver-offering', name: 'Offering' }];
const givingCode = frame => obj('shape', 'Giving code (picture)', { shape: 'rect', fill: 'repeating-conic-gradient(#111 0 25%, #fff 0 50%) 0 0 / 25% 25%, #fff', frame });
const giveBand = () => [obj('shape', 'Band', { shape: 'rect', fill: '#000000', opacity: .78, frame: { x: 0, y: 770, w: 1920, h: 310 } }), givingCode({ x: 1560, y: 800, w: 240, h: 240 }),
  obj('text', 'Giving words', { frame: { x: 120, y: 820, w: 1380, h: 110 }, style: { size: 70, bold: true, align: 'left' }, text: 'Scan to give' }),
  obj('text', 'Giving words, second line', { frame: { x: 120, y: 930, w: 1380, h: 80 }, style: { size: 42, align: 'left', color: '#b9b9b2' }, text: 'or give at the welcome desk' })];
const offerTwo = () => ({ id: newId('tpl'), name: 'Two languages', for: 'songs', version: 'ver-offering', objects: [box('Main', 'main', { x: 120, y: 110, w: 1680, h: 560 }, { size: 96, ...WORDS_LOOK }, 'The words of the song'), ...giveBand()] });
const offerOne = () => ({ id: newId('tpl'), name: 'One language', for: 'songs', version: 'ver-offering', objects: [box('Main', 'main', { x: 120, y: 110, w: 1680, h: 560 }, { size: 96, ...WORDS_LOOK }, 'The words of the song'), ...giveBand()] });
lowerThird.templates.push({ id: newId('tpl'), name: 'Words in a band', for: 'songs', version: 'ver-offering', objects: [
  obj('shape', 'Band', { shape: 'rect', fill: '#000000', opacity: .7, frame: { x: 0, y: 760, w: 1920, h: 320 } }),
  box('Main', 'main', { x: 120, y: 790, w: 1360, h: 240 }, { size: 58 }, 'The words of the song'), givingCode({ x: 1590, y: 790, w: 200, h: 200 }),
  obj('text', 'Giving words', { frame: { x: 1520, y: 1000, w: 340, h: 50 }, style: { size: 30, align: 'center' }, text: 'Scan to give' })] });
const mainTheme = { id: newId('theme'), name: 'Built-in', bg: '#000000', picture: null, builtIn: true, templates: [twoLang(), oneLang(), bibleTpl(), offerTwo(), offerOne()] };

/* ───────── stage layouts: boxes that show live things ───────── */
const link = (name, linked, frame, style, more) => obj('text', name, { linked, frame, style, text: '', ...more });
const stageLayouts = [
  { id: newId('lay'), name: 'Words and Next', objects: [
    link('Timer', 'timer', { x: 60, y: 30, w: 900, h: 90 }, { size: 64, align: 'left', color: '#6ee08a', shadow: false }, { timer: 'Sermon' }),
    link('Clock', 'clock', { x: 1320, y: 30, w: 540, h: 90 }, { size: 64, align: 'right', shadow: false }),
    link('Words', 'current', { x: 60, y: 140, w: 1800, h: 520 }, { size: 96, shadow: false }),
    link('Next', 'next', { x: 60, y: 720, w: 1800, h: 320 }, { size: 56, color: '#ffc83a', shadow: false })] },
  { id: newId('lay'), name: 'Words Only', objects: [link('Words', 'current', { x: 60, y: 60, w: 1800, h: 960 }, { size: 120, shadow: false })] },
  { id: newId('lay'), name: 'Words and Notes', objects: [
    link('Words', 'current', { x: 60, y: 60, w: 1800, h: 560 }, { size: 96, shadow: false }),
    link('Notes', 'notes', { x: 60, y: 660, w: 1800, h: 360 }, { size: 52, color: '#9ad0ff', align: 'left', shadow: false })] },
  { id: newId('lay'), name: 'Clock and Timer', objects: [
    link('Clock', 'clock', { x: 160, y: 160, w: 1600, h: 380 }, { size: 300, shadow: false }),
    link('Timer', 'timer', { x: 160, y: 580, w: 1600, h: 300 }, { size: 200, color: '#6ee08a', shadow: false }, { timer: 'Sermon' })] },
];

// NDI sources found on the network (the sample; the app's finder lists the real ones). The app's own outputs show greyed, as
// picking one would show itself.
const NDI_SOURCES = [
  { id: 'STREAM-PC (Camera 1)', name: 'Camera 1', host: 'STREAM-PC' },
  { id: 'STREAM-PC (Pulpit)', name: 'Pulpit', host: 'STREAM-PC' },
  { id: 'LOBBY-MAC (Lobby screen)', name: 'Lobby screen', host: 'LOBBY-MAC' },
  { id: 'SOFTPRESENTER (Audience)', name: 'Audience', host: 'This computer', own: true },
];

const DOC = {
  songs: [light, harbour, steady, evening, carol, notices],
  libraries: [{ id: newId('lib'), name: 'Christmas', songs: [carol.id] }, { id: newId('lib'), name: 'Youth', songs: [steady.id, evening.id] }],
  playlists: [
    { id: newId('pl'), name: 'Sunday Morning', items: [
      { id: newId('it'), kind: 'media', media: 'm5' },
      { id: newId('it'), kind: 'header', name: 'Worship' },
      { id: newId('it'), kind: 'song', song: light.id },
      { id: newId('it'), kind: 'song', song: harbour.id },
      { id: newId('it'), kind: 'header', name: 'The Word' },
      { id: newId('it'), kind: 'passage', passage: { book: 'John', chapter: 3, from: 16, to: 18 } },
      { id: newId('it'), kind: 'placeholder', name: 'Sermon slides', song: null },
      { id: newId('it'), kind: 'song', song: steady.id },
      { id: newId('it'), kind: 'mediaPlaylist', mp: 'mp1' },
    ] },
    { id: newId('pl'), name: 'Wednesday Evening', items: [
      { id: newId('it'), kind: 'song', song: evening.id },
      { id: newId('it'), kind: 'passage', passage: { book: 'Psalm', chapter: 23, from: 1, to: 3 } },
      { id: newId('it'), kind: 'song', song: steady.id },
      { id: newId('it'), kind: 'song', song: 'gone', title: 'Old Hymn 41' },
    ] },
  ],
  media: [
    { id: 'm1', name: 'Graphite.png', bg: MEDIA_BG.graphite, scaling: 'Scale to Fill' },
    { id: 'm2', name: 'Grid.png', bg: MEDIA_BG.grid, scaling: 'Scale to Fill' },
    { id: 'm3', name: 'Ember.png', bg: MEDIA_BG.ember, scaling: 'Scale to Fill' },
    { id: 'm4', name: 'Drift.mp4', bg: MEDIA_BG.drift, video: true, length: 94, loop: true, scaling: 'Scale to Fill' },
    { id: 'm5', name: 'Countdown.mp4', bg: MEDIA_BG.count, video: true, length: 300, loop: false, scaling: 'Scale to Fit' },
  ],
  mediaPlaylists: [{ id: 'mp1', name: 'Announcements', items: ['m2', 'm3', 'm4'], follow: null }],
  mediaTransition: { kind: 'Dissolve', time: .4 },
  themes: [mainTheme, lowerThird],
  versions: VERSIONS,
  mainTheme: mainTheme.id,
  stageLayouts,
  groups: [
    { name: 'Intro', color: '#9b59b6', key: 'I' }, { name: 'Verse 1', color: '#4285f4', key: 'A' }, { name: 'Verse 2', color: '#4285f4', key: 'S' }, { name: 'Verse 3', color: '#4285f4', key: 'D' },
    { name: 'Pre-chorus', color: '#34a86e', key: 'P' }, { name: 'Chorus', color: '#cc4eb4', key: 'C' }, { name: 'Bridge', color: '#f4a000', key: 'B' }, { name: 'Tag', color: '#78909c', key: 'T' }, { name: 'Ending', color: '#00acc1', key: 'E' },
  ],
  timers: [
    { id: newId('tm'), name: 'Sermon', kind: 'down', start: 1800, left: 1800, run: false },
    { id: newId('tm'), name: 'Service starts', kind: 'to', at: '10:30', left: 300, run: false },
    { id: newId('tm'), name: 'Since we began', kind: 'up', start: 0, left: 0, run: false },
  ],
  messages: [
    { id: newId('msg'), name: 'Phones', text: 'Please silence your phones', leaves: 0, look: 'Band' },
    { id: newId('msg'), name: 'Starting soon', text: 'Service begins in {timer:Service starts}', leaves: 0, look: 'Lower third' },
    { id: newId('msg'), name: 'Car lights', text: 'A car in the car park has its lights on', leaves: 20, look: 'Box' },
  ],
  props: [
    { id: newId('prop'), name: 'Church logo', clears: 0, objects: [obj('shape', 'Mark', { shape: 'rounded', fill: null, stroke: { w: 6, color: '#ffffff' }, frame: { x: 1600, y: 60, w: 250, h: 110 } }),
      obj('text', 'Name', { text: 'HARBOUR', frame: { x: 1600, y: 60, w: 250, h: 110 }, style: { size: 40, shadow: false } })] },
    { id: newId('prop'), name: 'Speaker name', clears: 30, objects: [obj('shape', 'Band', { shape: 'rect', fill: '#ff4a1a', frame: { x: 120, y: 860, w: 760, h: 120 } }),
      obj('text', 'Name', { text: 'Guest speaker', frame: { x: 150, y: 860, w: 700, h: 120 }, style: { size: 52, align: 'left', shadow: false } })] },
  ],
  macros: [
    { id: newId('mac'), name: 'Start of service', color: '#34c56b', atStart: false, steps: [{ do: 'clear', what: 'all' }, { do: 'audience', on: true }, { do: 'timer', what: 'start', name: 'Since we began' }, { do: 'media', id: 'm1' }] },
    { id: newId('mac'), name: 'Sermon', color: '#4285f4', atStart: false, steps: [{ do: 'clear', what: 'slide' }, { do: 'timer', what: 'start', name: 'Sermon' }, { do: 'prop', what: 'show', name: 'Speaker name' }] },
    { id: newId('mac'), name: 'End of service', color: '#ff4a1a', atStart: false, steps: [{ do: 'clear', what: 'all' }, { do: 'timer', what: 'stop', name: 'Sermon' }, { do: 'message', what: 'hide' }] },
  ],
  audio: [
    { id: newId('au'), name: 'Walk-in music', tracks: [{ name: 'Quiet Start', length: 214 }, { name: 'Open Doors', length: 187 }, { name: 'Gathering', length: 242 }] },
    { id: newId('au'), name: 'Walk-out music', tracks: [{ name: 'Go Well', length: 201 }] },
  ],
  screens: [
    { id: newId('scr'), kind: 'Audience', name: 'Main projector', purpose: 'room', to: 'Display: HP 27f', on: true, theme: null, layers: ['slide', 'media', 'props', 'messages', 'announcements'], colour: '#000000', edges: [0, 0, 0, 0], bgMode: 'solid', tint: '#4285f4' },
    { id: newId('scr'), kind: 'Audience', name: 'Stream', purpose: 'stream', to: 'NDI', ndiName: 'SoftPresenter Stream', receivers: 2, on: true, theme: lowerThird.id, layers: ['slide', 'props', 'messages'], colour: '#000000', edges: [0, 0, 0, 0], bgMode: 'clear', alpha: 'straight', tint: '#9b59b6' },
    { id: newId('scr'), kind: 'Audience', name: 'Lobby', purpose: 'lobby', to: 'Display: 3DTV', on: true, theme: null, layers: ['media', 'announcements'], colour: '#000000', edges: [58, 32, 58, 32], bgMode: 'solid', tint: '#f4a000', disconnected: true },
    { id: newId('scr'), kind: 'Stage', name: 'Confidence monitor', purpose: 'stage', to: 'Display: Stage TV', on: true, layout: stageLayouts[0].id, tint: '#34c56b' },
  ],
  settings: { sound: { main: 'default', music: { to: 'main', pair: 0 }, video: { to: 'main', pair: 0, level: 100 }, stream: { music: false, video: true } }, transition: { kind: 'Cut', time: .5, dir: 'Left' }, keys: { next: [' ', 'ArrowRight'], previous: ['ArrowLeft'], clearAll: MAC ? ['Escape'] : ['Escape', 'F1'], clearSlide: MAC ? [] : ['F2'], clearMedia: MAC ? [] : ['F3'], clearProps: MAC ? [] : ['F4'], clearAudio: MAC ? [] : ['F5'], clearMessages: MAC ? [] : ['F6'], find: ['/'] }, continuous: true, view: 'grid', size: 200, copyrightAt: 'first', ccliLicence: '0000000', remote: true },
  voxlive: { connected: true, key: 'vx_live_4f2a', screens: [
    { id: 'v1', name: 'Main captions', lang: 'Russian', mode: 'Captions', look: { size: 64, colour: '#ffffff', background: true, lines: 2 } },
    { id: 'v2', name: 'Scripture', lang: 'Ukrainian', mode: 'Scripture', look: { size: 52, colour: '#ffc400', background: false, lines: 3 } }] },
};

/* ───────── Bible reference data ───────── */
const BOOKS = [['GEN', 'Genesis', 50], ['EXO', 'Exodus', 40], ['LEV', 'Leviticus', 27], ['NUM', 'Numbers', 36], ['DEU', 'Deuteronomy', 34], ['JOS', 'Joshua', 24], ['JDG', 'Judges', 21], ['RUT', 'Ruth', 4], ['1SA', '1 Samuel', 31], ['2SA', '2 Samuel', 24], ['1KI', '1 Kings', 22], ['2KI', '2 Kings', 25], ['1CH', '1 Chronicles', 29], ['2CH', '2 Chronicles', 36], ['EZR', 'Ezra', 10], ['NEH', 'Nehemiah', 13], ['EST', 'Esther', 10], ['JOB', 'Job', 42], ['PSA', 'Psalm', 150], ['PRO', 'Proverbs', 31], ['ECC', 'Ecclesiastes', 12], ['SOL', 'Song of Solomon', 8], ['ISA', 'Isaiah', 66], ['JER', 'Jeremiah', 52], ['LAM', 'Lamentations', 5], ['EZE', 'Ezekiel', 48], ['DAN', 'Daniel', 12], ['HOS', 'Hosea', 14], ['JOE', 'Joel', 3], ['AMO', 'Amos', 9], ['OBA', 'Obadiah', 1], ['JON', 'Jonah', 4], ['MIC', 'Micah', 7], ['NAH', 'Nahum', 3], ['HAB', 'Habakkuk', 3], ['ZEP', 'Zephaniah', 3], ['HAG', 'Haggai', 2], ['ZEC', 'Zechariah', 14], ['MAL', 'Malachi', 4],
  ['MAT', 'Matthew', 28], ['MAR', 'Mark', 16], ['LUK', 'Luke', 24], ['JOH', 'John', 21], ['ACT', 'Acts', 28], ['ROM', 'Romans', 16], ['1CO', '1 Corinthians', 16], ['2CO', '2 Corinthians', 13], ['GAL', 'Galatians', 6], ['EPH', 'Ephesians', 6], ['PHI', 'Philippians', 4], ['COL', 'Colossians', 4], ['1TH', '1 Thessalonians', 5], ['2TH', '2 Thessalonians', 3], ['1TI', '1 Timothy', 6], ['2TI', '2 Timothy', 4], ['TIT', 'Titus', 3], ['PHM', 'Philemon', 1], ['HEB', 'Hebrews', 13], ['JAM', 'James', 5], ['1PE', '1 Peter', 5], ['2PE', '2 Peter', 3], ['1JO', '1 John', 5], ['2JO', '2 John', 1], ['3JO', '3 John', 1], ['JUD', 'Jude', 1], ['REV', 'Revelation', 22]]
  .map(([code, name, chapters]) => ({ code, name, chapters }));
// Other ways the sample books are typed, in English, Russian and Ukrainian. A Russian Psalm number is one less.
const BOOK_ALIASES = { jn: 'John', jhn: 'John', ps: 'Psalm', psa: 'Psalm', psalms: 'Psalm', gn: 'Genesis', rom: 'Romans', rm: 'Romans',
  'ин': 'John', 'иоанна': 'John', 'ів': 'John', 'івана': 'John', 'ин.': 'John', 'пс': 'Psalm*', 'псалом': 'Psalm*', 'быт': 'Genesis', 'бытие': 'Genesis', 'бут': 'Genesis', 'рим': 'Romans', 'римлянам': 'Romans' };
const BOOK_NAMES_RU = { Genesis: 'Бытие', Psalm: 'Псалом', John: 'От Иоанна', Romans: 'К Римлянам' };
// Russian (Synodal) names for all 66 books, full and short, in the order of BOOKS; what the operator reads when the Bible page is in Russian
const RU_FULL = ['Бытие', 'Исход', 'Левит', 'Числа', 'Второзаконие', 'Иисус Навин', 'Судьи', 'Руфь', '1 Царств', '2 Царств', '3 Царств', '4 Царств', '1 Паралипоменон', '2 Паралипоменон', 'Ездра', 'Неемия', 'Есфирь', 'Иов', 'Псалтирь', 'Притчи', 'Екклесиаст', 'Песнь песней', 'Исаия', 'Иеремия', 'Плач Иеремии', 'Иезекииль', 'Даниил', 'Осия', 'Иоиль', 'Амос', 'Авдий', 'Иона', 'Михей', 'Наум', 'Аввакум', 'Софония', 'Аггей', 'Захария', 'Малахия',
  'От Матфея', 'От Марка', 'От Луки', 'От Иоанна', 'Деяния', 'К Римлянам', '1 Коринфянам', '2 Коринфянам', 'К Галатам', 'К Ефесянам', 'К Филиппийцам', 'К Колоссянам', '1 Фессалоникийцам', '2 Фессалоникийцам', '1 Тимофею', '2 Тимофею', 'К Титу', 'К Филимону', 'К Евреям', 'Иакова', '1 Петра', '2 Петра', '1 Иоанна', '2 Иоанна', '3 Иоанна', 'Иуды', 'Откровение'];
const RU_SHORT = ['Быт', 'Исх', 'Лев', 'Чис', 'Втор', 'Нав', 'Суд', 'Руф', '1Цар', '2Цар', '3Цар', '4Цар', '1Пар', '2Пар', 'Езд', 'Неем', 'Есф', 'Иов', 'Пс', 'Прит', 'Еккл', 'Песн', 'Ис', 'Иер', 'Плач', 'Иез', 'Дан', 'Ос', 'Иоил', 'Ам', 'Авд', 'Ион', 'Мих', 'Наум', 'Авв', 'Соф', 'Агг', 'Зах', 'Мал',
  'Мф', 'Мк', 'Лк', 'Ин', 'Деян', 'Рим', '1Кор', '2Кор', 'Гал', 'Еф', 'Флп', 'Кол', '1Фес', '2Фес', '1Тим', '2Тим', 'Тит', 'Флм', 'Евр', 'Иак', '1Пет', '2Пет', '1Ин', '2Ин', '3Ин', 'Иуд', 'Откр'];
BOOKS.forEach((b, n) => { if (!BOOK_NAMES_RU[b.name]) BOOK_NAMES_RU[b.name] = RU_FULL[n]; });
const BIBLE_SAMPLE = 'Genesis 1, Psalm 23, John 1, John 3 and Romans 8';
const BIBLES_TO_DOWNLOAD = [['English', ['American Standard Version', "Young's Literal Translation", 'Geneva Bible', 'Darby Bible']], ['Deutsch', ['Luther 1912', 'Elberfelder 1905']], ['Français', ['Louis Segond', 'Darby (français)']],
  ['Español', ['Reina-Valera 1909', 'Versión Biblia Libre']], ['Português', ['Bíblia Livre']], ['Українська', ['Огієнка', 'Куліша']], ['Ἑλληνική', ['Robinson-Pierpont', 'SBL Greek New Testament']], ['עברית', ['Westminster Leningrad Codex']]];

/* ═════════ the Bible, until step 2 ═════════ */
const BIBLE_DATA = { bibles: { kjv: { name: 'King James Version', short: 'KJV', lang: 'en' }, syn: { name: 'Russian Synodal', short: 'RST', lang: 'ru' } }, text: { kjv: {}, syn: {} } };      // the Bible page waits for step 2 of the API

/* ═════════ the song catalog, until the app serves it ═════════ */
const CATALOG = { name: 'Worship Leader App', site: 'worshipleaderapp.com', langs: {}, songs: [] };      // the catalog comes with a later step of the API

/* ═════════ core.js ═════════ */
// Core: state, helpers, actions, menus and sheets, undo, drawing slides from objects and themes, and the show itself.
'use strict';

/* ───────── helpers ───────── */
const $ = id => document.getElementById(id);
const BR = html`<br>`;
const lines = s => html`${String(s == null ? '' : s).split('\n').map((l, n) => n ? [BR, l] : l)}`;
const two = n => String(n).padStart(2, '0');
const fmt = t => [Math.floor(t / 3600), Math.floor(t / 60) % 60, Math.floor(t % 60)].map(two).join(':');
const mmss = t => `${Math.floor(t / 60)}:${two(Math.floor(t % 60))}`;
const svg = (d, w = 1.4, s = 13) => html`<svg width="${s}" height="${s}" viewBox="0 0 13 13" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>`;
// A label that swaps between two wordings keeps the width of the longer one, so nothing beside it moves when it changes.
const sw = (on, a, b) => html`<span class="ui-sw" data-w="${on ? b : a}">${on ? a : b}</span>`;
const I = {
  undo: svg('M4.800 3L2.300 5.500 4.800 8M2.300 5.500h5.700a2.500 2.500 0 0 1 0 5H6.200', 1.4), redo: svg('M8.200 3l2.500 2.500L8.200 8M10.700 5.500H5a2.500 2.500 0 0 0 0 5h1.800', 1.4),
  note: svg('M5 10.300a1.700 1.700 0 1 1-1.700-1.700A1.700 1.700 0 0 1 5 10.300zM5 10.300V3.200l6-1.400v7.300M11 9a1.700 1.700 0 1 1-1.700-1.700A1.700 1.700 0 0 1 11 9z', 1.3),
  theme: svg('M6.500 2a4.500 4.500 0 1 0 0 9 4.500 4.500 0 0 0 0-9zM6.500 2v9M6.500 4.500h3.800M6.500 6.500h4.500M6.500 8.500h3.800', 1.2), template: svg('M2 3h9v7.500H2zM2 5.500h9M5.200 5.500v5', 1.2), layers: svg('M2 8.700l4.500 2 4.500-2M2 6l4.500 2 4.500-2-4.500-2z', 1.2), monitor: svg('M2 2.800h9v6H2zM4.800 11h3.400M6.500 8.800V11', 1.2),
  playlist: svg('M2 3.5h.01M2 6.5h.01M2 9.5h.01M4.800 3.500h6.200M4.800 6.500h6.200M4.800 9.500h6.200', 1.6), library: svg('M2.800 2.500v7.500M5.500 2.500v7.500M8 3.200l2.300 6.800M1.500 11h10', 1.5),
  list: svg('M2 3.5h9M2 6.5h9M2 9.5h9'), pen: svg('M2 11l.6-2.6L9 2l2 2-6.4 6.4z', 1.3), chev: svg('M3 5l3.5 3.5L10 5', 1.6), more: svg('M3 6.5h.01M6.5 6.5h.01M10 6.5h.01', 2.4),
  plus: svg('M6.5 2.5v8M2.5 6.5h8', 1.6), x: svg('M3 3l7 7M10 3l-7 7', 1.6), check: svg('M2.5 7l2.8 2.8L10.500 3.500', 1.8), back: svg('M8 2.5L4.5 6.500 8 10.500', 1.8),
  play: html`<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor"><path d="M3 1.5v9l7.5-4.5z"/></svg>`,
  pause: html`<svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor"><rect x="2.5" y="2" width="2.5" height="8"/><rect x="7" y="2" width="2.5" height="8"/></svg>`,
  reset: svg('M2.5 6.5A4 4 0 1 0 6.5 2.5M8 1L6.5 2.5 8 4'), prev: svg('M8 2.5L4.5 6.5 8 10.500', 1.8), next: svg('M5 2.5l3.500 4-3.500 4', 1.8), fwd: svg('M10.5 6.5A4 4 0 1 1 6.5 2.5M5 1l1.5 1.5L5 4'),      // forward 10 seconds: a circle turning clockwise, open at the upper right, its head at the top pointing right; reset turns the other way (as built, 24 November)
  info: svg('M6.5 6v3.500M6.500 3.700h.01M6.500 1.500a5 5 0 1 0 0 10 5 5 0 0 0 0-10z'), words: svg('M2 3h9M2 6h9M2 9h5.500'), arr: svg('M2 4h6.500M2 9h6.500M9 2.500L11 4 9 5.500M9 7.500L11 9l-2 1.500'), notes: svg('M3 2h7v9H3zM5 4.500h3M5 6.500h3M5 8.500h2'),
  grid: svg('M2 2h3.700v3.700H2zM7.300 2H11v3.700H7.300zM2 7.300h3.700V11H2zM7.300 7.300H11V11H7.300z', 1.2), easy: svg('M2 3.500h9M2 6.500h9M2 9.500h9'), table: svg('M2 2.500h9v8H2zM2 5.500h9M5.500 2.500v8', 1.2),
  left: svg('M2 3h9M2 6.5h5.5M2 10h7.500'), center: svg('M2 3h9M3.800 6.500h5.400M2.800 10h7.400'), right: svg('M2 3h9M5.500 6.500H11M3.500 10H11'),
  top: svg('M2 2h9M6.500 11V5M4 7.500L6.500 5 9 7.500'), middle: svg('M2 6.500h9M6.500 1.500v3M6.500 11.500v-3'), bottom: svg('M2 11h9M6.500 2v6M4 5.500L6.500 8 9 5.500'),
  song: svg('M2 3h9v7.500H2zM4 5.500h5M4 7.800h3', 1.2), book: svg('M6.500 3.500C5 2.500 3.200 2.500 1.500 3v7.500c1.700-.500 3.500-.500 5 .500 1.500-1 3.300-1 5-.500V3c-1.700-.500-3.500-.500-5 .500zM6.500 3.500V11', 1.2),
  film: svg('M2 2.500h9v8H2zM4.200 2.500v8M8.800 2.500v8M2 6.500h2.200M8.800 6.500H11', 1.2), loop: svg('M3 5a3.500 3.500 0 0 1 6.500-1M10 8a3.500 3.500 0 0 1-6.500 1M9.500 2v2H7.500M3.500 11V9h2'), slot: svg('M2.500 2.500h8v8h-8z', 1.2),
  sound: svg('M2 5v3h2l3 2.500v-8L4 5zM9 4.500a3 3 0 0 1 0 4'), mute: svg('M2 5v3h2l3 2.500v-8L4 5zM9 5l2.500 3M11.500 5L9 8'),
  eye: svg('M1.500 6.500s2-3.500 5-3.500 5 3.500 5 3.500-2 3.500-5 3.500-5-3.500-5-3.500zM6.500 5a1.500 1.500 0 1 0 0 3 1.500 1.500 0 0 0 0-3z', 1.2), lock: svg('M3.500 6h6v5h-6zM4.800 6V4.300a1.700 1.700 0 0 1 3.400 0V6', 1.2),
};

let toastTimer;
/* ───────── notices: one system (the owner, 22 November: "we need to fix how any notice shows up") ─────────
   Where: one line in the top bar's middle, between the find box and the clock, so it never covers the previews, the slides
   or the media strip, and nothing moves when it comes or goes. One at a time; a newer one takes the place of the older.
   What earns one: trouble (amber dot), a refusal that says why nothing happened, and things that happen out of sight (added
   to another playlist, copied, a file read or written, the app's answers). Never a confirmation of what the operator just
   did and can see (undone, opened, removed: Undo is the way back).
   How long: information 4 s; trouble 8 s; one that needs an answer (a song changed outside) stays until answered. Every
   notice has an × that is always there; clicking an action answers it. */
// The owner chose this on 22 November ("i like that idea better it saves on real estate and has all messages in one place"):
// a status button in the top bar's middle, and the list it opens. Trouble and questions are counted in amber; what happens
// out of sight goes in the list with a quiet dot; a refusal is a brief cue at the place of action, never an entry.
const NOTES = []; let noteSeq = 0;      // the list, newest first
function toast(text, o = {}) { NOTES.unshift({ id: ++noteSeq, kind: o.kind || 'info', text, time: new Date(), seen: false, action: o.action || null }); if (NOTES.length > 40) NOTES.length = 40; renderNote(); }
const trouble = (text, o = {}) => toast(text, { ...o, kind: 'trouble' });
// A refusal (why nothing happened): a brief cue under what was clicked, or under the preview's readout when it was a key
let lastAct = { rect: null, at: 0 };
function refuse(text) {
  const near = Date.now() - lastAct.at < 600 && lastAct.rect, foot = document.querySelector('#pgFoot .txtline') || document.querySelector('#pgFoot');
  const r = near || (foot && foot.getBoundingClientRect()); let h = $('hint');
  if (!h) { h = document.createElement('div'); h.id = 'hint'; h.className = 'hint'; h.setAttribute('role', 'status'); document.body.appendChild(h); }
  h.textContent = text; h.classList.add('on');
  const w = Math.min(360, innerWidth - 20); h.style.maxWidth = w + 'px';
  if (r) { h.style.left = Math.max(10, Math.min(r.left, innerWidth - w - 10)) + 'px'; h.style.top = Math.min(r.bottom + 6, innerHeight - 50) + 'px'; }
  clearTimeout(h._t); h._t = setTimeout(() => h.classList.remove('on'), 2400);
}
// questions waiting for an answer: kept in place until answered (today, a song changed outside SoftPresenter)
const openAsks = () => { if (typeof REMOTE !== 'undefined' && REMOTE.on) return remoteAsks(); const s = state.changedOutside && songById(state.changedOutside);
  // a device that wants to show its screen (Ask me each time): nothing of its picture is taken before Allow
  const m = state.mirrorAsk ? [{ kind: 'ask', text: `${state.mirrorAsk} wants to show its screen.`, actions: [['Allow', 'mirror:allow'], ['Don’t Allow', 'mirror:deny']] }] : [];
  return [...m, ...(s ? [{ kind: 'ask', html: html`<b>${s.title}</b> was changed outside SoftPresenter. Read it again from its file, or keep SoftPresenter’s version and write it over the file?`, actions: [['Reload', 'outside:reload'], ['Keep mine', 'outside:keep']] }] : [])]; };
// what the slot shows
function renderNote() {
  const el = $('note'); if (!el) return;
    const asks = openAsks(), unseen = NOTES.filter(n => !n.seen), bad = asks.length + unseen.filter(n => n.kind === 'trouble').length, info = unseen.length - unseen.filter(n => n.kind === 'trouble').length;
    el.className = 'note nb';
    put(el, html`<button class="nbtn ${bad ? 'warn' : info ? 'new' : ''} ${state.notesOpen ? 'open' : ''}" data-a="notesOpen" aria-label="Notices${bad ? `: ${bad} need${bad === 1 ? 's' : ''} you` : info ? ': new' : ''}" title="${asks.length ? 'A question is waiting: click to answer it' : bad ? 'Something went wrong: click to see' : info ? 'New notices' : 'Notices'}">${svg('M3 4.500h7M3 7h7M3 9.500h4.500', 1.3)}${bad ? html`<b>${asks.length && !unseen.some(n => n.kind === 'trouble') ? '?' : bad}</b>` : info ? html`<i class="nd"></i>` : ''}</button>`);
    renderNotesPanel();
}
// The list: a panel under the button, newest first, every notice's full text, its time and its buttons; questions first.
// Opening it marks everything seen; Clear All empties it (a question stays until answered).
function renderNotesPanel() {      // times in the computer's 12- or 24-hour way, no AM/PM, like the clock
  let p = $('notesPanel'); if (!state.notesOpen) { if (p) p.remove(); return; }
  if (!p) { p = document.createElement('div'); p.id = 'notesPanel'; p.className = 'notes'; p.setAttribute('role', 'dialog'); p.setAttribute('aria-label', 'Notices'); document.body.appendChild(p); }
  const b = document.querySelector('.nbtn'), r = b && b.getBoundingClientRect(), w = 380; p.style.left = r ? Math.max(10, Math.min(r.left + r.width / 2 - w / 2, innerWidth - w - 10)) + 'px' : '50%'; p.style.top = r ? r.bottom + 8 + 'px' : '60px';
  const asks = openAsks(), hm = d => d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }).replace(/\s?[AaPp]\.?[Mm]\.?$/, ''), mark = k => html`<i class="nk ${k}">${k === 'ask' ? '?' : ''}</i>`;
  const row = n => html`<div class="nrow ${n.kind} ${n.seen ? 'seen' : ''}">${mark(n.kind)}<div class="nt"><p>${n.html || n.text}</p>${(n.actions || (n.action ? [n.action] : [])).length ? html`<div class="nacts">${(n.actions || [n.action]).map(([t, a], k) => html`<button class="btn sm ${k === 0 && n.kind === 'ask' ? 'primary' : ''}" data-a="${a}">${t}</button>`)}</div>` : ''}</div>${n.time ? html`<span class="ntime mono">${hm(n.time)}</span>` : ''}</div>`;
  put(p, html`<div class="nhead"><b>Notices</b><span class="sp"></span><button class="btn sm" data-a="notesClear" ${{ disabled: !NOTES.length }}>Clear All</button></div>
    <div class="nlist">${asks.map(row)}${NOTES.length ? NOTES.map(row) : asks.length ? '' : html`<p class="nnone">Nothing to tell you. Trouble, questions, and what happens out of sight (added to another playlist, a file read or written) come here.</p>`}</div>`);
}
document.addEventListener('mousedown', e => { if (state.notesOpen && !e.target.closest('#notesPanel, .nbtn')) A.notesOpen(); });      // a click elsewhere closes it

/* ───────── state: what this browser is looking at, and what the screens hold ───────── */
const state = {
  mode: 'show',                               // show | bible | edit | themes | settings
  src: { type: 'pl', id: DOC.playlists[0].id }, srcMenu: false, filter: '',
  openItem: null, openSong: DOC.songs[1].id, picked: { deck: null, set: [] }, itemPick: [], itemAnchor: null, itemCursor: null, heldDeck: null, songPick: [], mediaPick: [], arrOpen: {},
  live: null,                                 // { deck, q, s }: where the show is
  on: null,                                   // what the screens hold: { slide, songId, kind, title, group, n, total }
  mediaOn: null, mediaPl: null, video: { playing: true, pos: 0, loop: true, mute: true },
  build: 0, cue: null, propsOn: [], message: null, messageLeft: 0, ann: null, vox: null, advanceLeft: 0,
  audio: { pl: null, track: 0, pos: 0, playing: false, volume: 80, through: true, shuffle: false, crossfade: 3 },
  audience: true, stage: true, monitor: 'Audience', tab: 'Timers', stageMsg: '', test: 'off', identify: false,
  bible: { ref: { book: 'John', chapter: 3, from: 16, to: 18 }, rows: [{ bible: 'syn', verse: 'verse1', ref: 'ref1' }, { bible: 'kjv', verse: 'verse2', ref: 'ref2' }],
    opts: { per: 'verse', numbers: false, name: false, refAt: 'every', theme: null }, view: 'page', lists: true, fold: true, side: 'books', history: [], installed: [], hint: [] },
  edit: { target: null, k: 0 }, sels: [], itab: 'Text', reflow: false, liveEdit: false, zoom: 'Fit', gridOn: false,
  themeSel: { theme: DOC.mainTheme, tpl: null },
  section: 'Screens', menu: null, sheet: null, clip: null, changedOutside: null,
};
state.openItem = DOC.playlists[0].items[3].id;

/* ───────── lookups ───────── */
const byId = (list, id) => list.find(x => x.id === id);
const songById = id => byId(DOC.songs, id);
const mediaById = id => byId(DOC.media, id);
const playlist = () => byId(DOC.playlists, state.src.id);
const groupColour = name => { const g = DOC.groups.find(x => x.name === name) || DOC.groups.find(x => name.toLowerCase().startsWith(x.name.toLowerCase().split(' ')[0])); return g ? g.color : '#808080'; };
const slideWords = (s, role) => (s.objects.find(o => o.role === (role || 'main')) || s.objects.find(o => o.kind === 'text' && !o.linked) || { text: '' }).text || '';
const allWords = s => s.objects.filter(o => o.kind === 'text').map(o => o.text).join('\n');

/* ───────── actions: every control names one, so there is one click handler ───────── */
const A = {}, C = {}, M = {};
function act(str, el, ev) {
  const n = str.indexOf(':'), name = n < 0 ? str : str.slice(0, n), args = n < 0 ? [] : str.slice(n + 1).split(',');
  if (!A[name]) return console.warn('No action called', name);
  return A[name](args, el, ev);
}

// A menu: [{ t: label, f: what it does, on: ticked, sub: [...], sep: true, dim: true }]
function openMenu(items, x, y) { state.menu = { stack: [items.filter(Boolean)], x, y }; renderMenu(); }
function closeMenu() { if (state.menu) { state.menu = null; renderMenu(); } }
function renderMenu() {
  const m = state.menu, el = $('menu');
  if (!m) { el.hidden = true; el.replaceChildren(); return; }
  const items = m.stack[m.stack.length - 1];
  el.hidden = false;
  put(el, m.stack.length > 1 ? html`<button class="ui-mi back" data-a="menuBack">${I.back} Back</button>` : '',
    items.map((it, n) => it.sep ? html`<i class="ui-sep"></i>` : it.head ? html`<span class="ui-mh">${it.head}</span>`
      : html`<button class="ui-mi ${it.dim ? 'dim' : ''}" data-a="menuPick:${n}"><span class="ck">${it.on ? I.check : ''}</span><span class="t">${it.t}</span>${it.sub ? html`<span class="go">${I.next}</span>` : it.k ? html`<kbd>${keyText(it.k)}</kbd>` : ''}</button>`));
  const w = 232, h = el.offsetHeight;
  el.style.left = Math.max(8, Math.min(m.x, innerWidth - w - 8)) + 'px';
  el.style.top = Math.max(8, Math.min(m.y, innerHeight - h - 8)) + 'px';
  const first = el.querySelector('.ui-mi:not(.dim):not(.back)'); if (first) first.focus({ preventScroll: true });      // so the arrows and Enter work in it at once
}
A.menuBack = () => { state.menu.stack.pop(); renderMenu(); };
A.menuPick = ([n]) => {
  const it = state.menu.stack[state.menu.stack.length - 1][+n];
  if (it.dim) return;
  if (it.sub) { state.menu.stack.push(it.sub.filter(Boolean)); return renderMenu(); }
  closeMenu(); if (it.f) it.f();
};
A.menu = ([kind, ...args], el) => { const r = el.getBoundingClientRect(); openMenu(M[kind](args), r.left, r.bottom + 4); };

// A sheet: a small window over the page. { title, body, buttons: [{ t, f, primary }], wide }
function openSheet(sh) { state.sheet = sh; renderSheet(); const first = $('sheet').querySelector('input, textarea, select'); if (first) first.focus(); }
function closeSheet() { state.sheet = null; state.capture = null; renderSheet(); }
function renderSheet() {
  const s = state.sheet, el = $('sheet');
  if (!s) { el.hidden = true; el.replaceChildren(); return; }
  el.hidden = false;
  setTimeout(() => fitAll(el), 0);
  put(el, html`<div class="ui-sheet ${s.wide ? 'wide' : ''} ${s.cls || ''}"><div class="ui-sh"><b>${s.title}</b><button class="iconbtn" data-a="sheetClose" aria-label="Close">${I.x}</button></div>
    <div class="ui-sb">${s.body}</div><div class="ui-sf">${s.note ? html`<span class="sub">${s.note}</span>` : html`<span></span>`}
      ${(s.buttons || [{ t: 'Done' }]).map((b, n) => html`<button class="btn ${b.primary ? 'primary' : ''}" data-a="sheetBtn:${n}">${b.t}</button>`)}</div></div>`);
}
A.sheetClose = () => closeSheet();
A.sheetBtn = ([n]) => { const b = (state.sheet.buttons || [{ t: 'Done' }])[+n]; if (b.f && b.f() === false) return; if (!b.keep) closeSheet(); };
function ask(title, label, value, done, more = {}) {
  openSheet({ title, body: html`<label class="ui-f"><span class="cap">${label}</span><input class="field" id="askVal" value="${value}" ${{ placeholder: more.placeholder || null }}></label>${more.extra || ''}`,
    note: more.note, buttons: [{ t: 'Cancel' }, { t: more.ok || 'OK', primary: true, f: () => { const v = $('askVal').value.trim(); if (!v && !more.allowEmpty) return false; done(v); } }] });
}
// Undo covers it, so it goes at once: no question and no notice. (Only what can't be undone asks, with confirmDo.)
const undoable = (_q, _w, _b, done) => done();
function confirmDo(title, text, ok, done) { openSheet({ title, body: html`<p class="ui-p">${text}</p>`, buttons: [{ t: 'Cancel' }, { t: ok, primary: true, f: done }] }); }
const field = (label, inner) => html`<label class="ui-f"><span class="cap">${label}</span>${inner}</label>`;
const options = (list, now) => html`${list.map(x => { const [v, t] = Array.isArray(x) ? x : [x, x]; return html`<option value="${v}" ${{ selected: String(v) === String(now) }}>${t}</option>`; })}`;
// more: { value: attributes for that one button }, such as a reason it is greyed
const seg = (list, now, action, more = {}) => html`<div class="seg">${list.map(x => { const [v, t] = Array.isArray(x) ? x : [x, x]; return html`<button class="${String(v) === String(now) ? 'on' : ''}" data-a="${action}:${v}" ${more[v] || null}>${t}</button>`; })}</div>`;

/* ───────── undo: every change to what is saved goes through mutate ───────── */
let undoStack = [], redoStack = [];
function restore(json) {      // timers and props that are counting keep counting: only what was edited goes back
  const d = JSON.parse(json), running = new Map([...DOC.timers, ...DOC.props].map(x => [x.id, x]));
  for (const k of Object.keys(DOC)) delete DOC[k]; Object.assign(DOC, d);
  for (const t of DOC.timers) { const was = running.get(t.id); if (was) { t.left = was.left; t.run = was.run; } }
  for (const p of DOC.props) { const was = running.get(p.id); if (was && was.left != null) p.left = was.left; }
}
function mutate(fn) {
  undoStack.push(JSON.stringify(DOC)); if (undoStack.length > 80) undoStack.shift(); redoStack = [];
  fn();
  if (state.mode === 'edit' && state.edit.target && state.edit.target.type === 'slide' && typeof catTouched === 'function') catTouched(songById(state.edit.target.songId));      // a song from the catalog, changed here, is never updated from it
  afterChange();
}
// After a change or an undo, anything this page was pointing at may be gone: point it somewhere that exists.
function fixState() {
  deckCache = null;
  if (state.monScreen && !byId(DOC.screens, state.monScreen)) state.monScreen = null;
  if (state.cue && !deckByKey(state.cue.deck)) state.cue = null;
  if (state.live && state.live.deck === 'bible-held' && !state.heldBible) state.live = null;
  if (!DOC.settings.sound) DOC.settings.sound = { main: 'default', music: { to: 'main', pair: 0 }, video: { to: 'main', pair: 0, level: 100 }, stream: { music: false, video: true } };
  if (state.src.type === 'pl' && !byId(DOC.playlists, state.src.id)) state.src = { type: 'pl', id: DOC.playlists[0].id };
  if (state.src.type === 'lib' && state.src.id !== 'all' && !byId(DOC.libraries, state.src.id)) state.src = { type: 'lib', id: 'all' };
  if (state.src.type === 'cat' && !catOn()) state.src = { type: 'lib', id: 'all' };
  if (state.src.type === 'vox') state.src = { type: 'pl', id: DOC.playlists[0].id };
  if (!songById(state.openSong)) state.openSong = (DOC.songs[0] || {}).id;
  if (state.mediaView && !byId(DOC.mediaPlaylists, state.mediaView)) state.mediaView = null;
  if (state.mediaOn && !mediaById(state.mediaOn)) state.mediaOn = null;
  state.songPick = state.songPick.filter(songById); state.mediaPick = state.mediaPick.filter(mediaById);
  if (state.message && !byId(DOC.messages, state.message)) state.message = null;
  state.propsOn = state.propsOn.filter(id => byId(DOC.props, id));
  if (state.ann && !(state.ann.song ? songById(state.ann.song) : annMedia())) state.ann = null;
  const t = state.edit.target;
  if (t && t.type === 'slide' && !songById(t.songId)) state.edit.target = { type: 'slide', songId: DOC.songs[0].id, k: 0 };
  if (t && t.type === 'prop' && !byId(DOC.props, t.id)) state.edit.target = { type: 'prop', id: (DOC.props[0] || {}).id };
  if (t && t.type === 'layout' && !byId(DOC.stageLayouts, t.id)) state.edit.target = { type: 'layout', id: DOC.stageLayouts[0].id };
  if (!byId(DOC.themes, state.themeSel.theme)) state.themeSel = { theme: DOC.mainTheme, tpl: null };
}
function afterChange() {   // the screens are left alone unless Live Edit is on
  fixState();
  if (state.live && state.layers_slide) {
    const d = deckByKey(state.live.deck);
    if (!d || !d.slides) { state.live = null; }
    else { state.live.q = Math.min(state.live.q, d.seq.length - 1); if (state.liveEdit) putUp(); }
  }
  render();
}
A.notesOpen = () => { state.notesOpen = !state.notesOpen; renderNote(); if (!state.notesOpen) { NOTES.forEach(n => { n.seen = true; }); renderNote(); } };      // seen once looked at, when the list closes
A.notesClear = () => { NOTES.length = 0; renderNote(); };
A.undo = () => { const u = undoStack.pop(); if (!u) return refuse('Nothing to undo'); redoStack.push(JSON.stringify(DOC)); restore(u); state.sels = []; afterChange();  };
A.redo = () => { const u = redoStack.pop(); if (!u) return refuse('Nothing to redo'); undoStack.push(JSON.stringify(DOC)); restore(u); state.sels = []; afterChange();  };

/* ───────── themes: a slide's box takes its place and look from the template unless it sets its own ───────── */
const themeFor = (song, themeId) => byId(DOC.themes, themeId || (song && song.theme) || DOC.mainTheme) || DOC.themes[0];
function templateFor(slide, song, kind, themeId, version) {
  const th = themeFor(song, themeId), fits = t => t.for === kind || t.for === 'both';
  const inVer = version && th.templates.filter(t => t.version === version && fits(t));      // a version wins over a slide's own template: changing the look is its point
  if (!(inVer && inVer.length) && slide.template) { const t = byId(th.templates, slide.template); if (t && !t.version) return t; }
  const fit = inVer && inVer.length ? inVer : th.templates.filter(t => !t.version && fits(t));
  if (kind === 'songs') { const two = slide.objects.some(o => o.role === 'translation' && o.text); return fit.find(t => t.objects.some(o => o.role === 'translation') === two) || fit[0] || th.templates[0]; }
  return fit[0] || th.templates[0];
}
const full = o => ({ ...o, frame: o.frame || { x: 460, y: 440, w: 1000, h: 200 }, style: o.kind === 'text' ? { ...DEF_STYLE, ...(o.style || {}) } : undefined });
function resolved(slide, ctx) {
  if (ctx.raw) return { bg: ctx.bg || '#000000', objects: slide.objects.map(full) };
  const th = themeFor(ctx.song, ctx.theme), tpl = templateFor(slide, ctx.song, ctx.kind || 'songs', ctx.theme, ctx.version);
  const boxes = tpl.objects.filter(o => o.role);
  return { bg: th.bg, picture: th.picture,
    objects: [...tpl.objects.filter(o => !o.role).map(o => ({ ...full(o), fromTheme: true })),
      ...slide.objects.filter(o => !(tpl.version && o.role && !boxes.some(x => x.role === o.role))).map(o => { const b = o.role && boxes.find(x => x.role === o.role);      // in a version, words with no box of theirs (the translation, say) are left off
        return { ...o, frame: o.frame || (b && b.frame) || { x: 120, y: 340, w: 1680, h: 400 }, style: o.kind === 'text' ? { ...DEF_STYLE, ...(b ? b.style : {}), ...(o.style || {}) } : undefined }; })] };
}

const CLIP = { triangle: 'polygon(50% 0,100% 100%,0 100%)', rhombus: 'polygon(50% 0,100% 50%,50% 100%,0 50%)', hexagon: 'polygon(25% 0,75% 0,100% 50%,75% 100%,25% 100%,0 50%)',
  star: 'polygon(50% 0,61% 35%,98% 35%,68% 57%,79% 91%,50% 70%,21% 91%,32% 57%,2% 35%,39% 35%)', arrow: 'polygon(0 30%,62% 30%,62% 0,100% 50%,62% 100%,62% 70%,0 70%)' };
const fillCSS = o => !o.gradient ? (o.fill || 'transparent') : o.gradient.kind === 'radial' ? `radial-gradient(circle, ${o.fill || '#000'}, ${o.gradient.to})` : `linear-gradient(${o.gradient.angle}deg, ${o.fill || '#000'}, ${o.gradient.to})`;

function linkedValue(o, ctx) {
  const now = new Date(), tm = DOC.timers.find(t => t.name === o.timer) || DOC.timers[0];
  switch (o.linked) {
    case 'clock': return now.toLocaleTimeString('en-GB').slice(0, 5);
    case 'date': return now.toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long' });
    case 'timer': return tm ? `${tm.name}  ${fmt(tm.left)}` : '';
    case 'title': return ctx.title || (ctx.song ? ctx.song.title : 'Song title');
    case 'group': return ctx.slide ? ctx.slide.group || '' : '';
    case 'label': return ctx.slide ? ctx.slide.label || '' : '';
    case 'notes': return ctx.notes != null ? ctx.notes : ctx.slide ? ctx.slide.notes || '' : '';
    case 'next': return ctx.nextWords != null ? ctx.nextWords : 'The next slide’s words';
    case 'current': return ctx.currentWords != null ? ctx.currentWords : 'The words on screen';
    case 'stagemsg': return state.stageMsg;
    case 'copyright': return copyrightLine(ctx.song);
  }
  return '';
}
function copyrightLine(song) {
  if (!song) return '';
  const i = song.info || {}, parts = [];
  if (i.author) parts.push(i.author); if (i.year || i.publisher) parts.push(`© ${[i.year, i.publisher].filter(Boolean).join(' ')}`);
  if (i.ccli) parts.push(`CCLI Song # ${i.ccli}`); if (DOC.settings.ccliLicence) parts.push(`CCLI Licence # ${DOC.settings.ccliLicence}`);
  if (song.from && DOC.settings.catCredit && typeof CATALOG !== 'undefined') parts.push(`Songs: ${CATALOG.name}`);      // only if the church chooses it (Settings › Integrations)
  return parts.join(' · ');
}

// The app makes text that doesn't fit its box smaller until it does (and, if asked, bigger until it fills it).
// Here the real letters are measured after they are drawn.
function fitAll(root) {
  for (const el of root.querySelectorAll('.ob.text[data-fit]')) {
    const inner = el.firstElementChild; if (!el.clientHeight || inner.isContentEditable) continue;
    const [base, mode] = el.dataset.fit.split(','); let size = +base;
    const over = () => inner.scrollHeight > el.clientHeight + 1 || inner.scrollWidth > el.clientWidth + 1;
    el.style.fontSize = size + 'cqw';
    if (mode.includes('down')) for (let n = 0; n < 40 && over(); n++) { size *= .93; el.style.fontSize = size + 'cqw'; }
    if (mode.includes('up') && !over()) { for (let n = 0; n < 16 && !over(); n++) { size *= 1.07; el.style.fontSize = size + 'cqw'; } size /= 1.07; el.style.fontSize = size + 'cqw'; }
  }
}
// The clicks a slide's builds take: each object that comes on with a click (a paragraph at a time if asked), then each that goes off.
function buildSteps(slide) {
  const out = [];
  for (const o of slide.objects) { const b = o.build; if (b && b.in && b.in !== 'None' && /click/i.test(b.when || '')) { const units = b.byPara && o.kind === 'text' ? Math.max(1, (o.text || '').split('\n').length) : 1; for (let u = 0; u < units; u++) out.push({ id: o.id, phase: 'in' }); } }
  for (const o of slide.objects) if (o.build && o.build.out && o.build.out !== 'None') out.push({ id: o.id, phase: 'out' });
  return out;
}
/* ───────── words styled inside a box (the owner, 23 November) ───────── */
// A box's words are plain text (o.text); o.spans says where some of them differ from the box: { from, to, ...look }, offsets
// being this page's own string positions (the app's unit is turned into these where its data comes in). A span's look overrides
// the box's style, which overrides the theme. Alignment, spacing, bullets and the rest stay the box's.
const WORD_LOOK = ['font', 'size', 'bold', 'italic', 'underline', 'strike', 'color'];
// The words cut where any span starts or ends (and at extra cuts), each piece with the look its spans give it.
function runsOf(text, spans, cuts = []) {
  const at = new Set([0, text.length]), len = text.length;
  for (const n of [...cuts, ...(spans || []).flatMap(sp => [sp.from, sp.to])]) if (n > 0 && n < len) at.add(n);
  const edges = [...at].sort((a, b) => a - b), out = [];
  for (let k = 0; k + 1 < edges.length; k++) {
    const from = edges[k], to = edges[k + 1], look = {};
    for (const sp of spans || []) if (sp.from <= from && sp.to >= to) for (const p of WORD_LOOK) if (sp[p] !== undefined) look[p] = sp[p];
    out.push({ from, to, look: Object.fromEntries(WORD_LOOK.filter(p => p in look).map(p => [p, look[p]])) });
  }
  return out;
}
const sameLook = (a, b) => WORD_LOOK.every(p => a[p] === b[p]);
// Spans kept tidy: no overlaps, neighbours with the same look joined, and nothing that only repeats the box (a word set back to
// the box's look follows the box again).
function tidySpans(text, spans, box) {
  const out = [];
  for (const r of runsOf(text, spans)) {
    const look = {}; for (const p of WORD_LOOK) if (r.look[p] !== undefined && !(box && box[p] === r.look[p])) look[p] = r.look[p];
    if (!Object.keys(look).length) continue;
    const last = out[out.length - 1];
    if (last && last.to === r.from && sameLook(last, look)) last.to = r.to; else out.push({ from: r.from, to: r.to, ...look });
  }
  return out;
}
// Words retyped elsewhere (the words editor): spans over the unchanged start and end keep their words; the changed middle takes a
// span's look only from one that covered all of it.
function remapSpans(was, now, spans) {
  if (!spans || !spans.length || was === now) return spans;
  let pre = 0; while (pre < was.length && pre < now.length && was[pre] === now[pre]) pre++;
  let suf = 0; while (suf < was.length - pre && suf < now.length - pre && was[was.length - 1 - suf] === now[now.length - 1 - suf]) suf++;
  const d = now.length - was.length, oldEnd = was.length - suf;
  const start = x => x >= oldEnd ? x + d : x <= pre ? x : now.length - suf, end = x => x <= pre ? x : x >= oldEnd ? x + d : pre;      // words put in where a span starts or ends stay outside it
  return tidySpans(now, spans.map(sp => ({ ...sp, from: start(sp.from), to: end(sp.to) })).filter(sp => sp.to > sp.from));
}
// New words for a box (the words editor): its spans follow the words that stayed.
function setWords(o, text) { const sp = remapSpans(o.text || '', text, o.spans); o.text = text; if (sp && sp.length) o.spans = sp; else delete o.spans; }
const decoCSS = l => [l.underline && 'underline', l.strike && 'line-through'].filter(Boolean).join(' ') || 'none';
// One piece's look, against its box: the size as a share of the box's, so fitting the box to its words shrinks every piece alike.
const runCSS = (look, s) => { const l = { ...s, ...look };
  return `font-family:${FONTS[l.font] || FONTS[DEF_FONT]};font-size:${+(l.size / s.size).toFixed(4)}em;font-weight:${l.bold ? 700 : 400};font-style:${l.italic ? 'italic' : 'normal'};text-decoration:${decoCSS(l)};color:${l.color};`; };
// The words of a box drawn piece by piece. Each piece carries its look (data-look) so that what is typed can be read back with
// its styling; a bullet or number is drawn but not typed (data-pre). hl: the words picked while the inspector is being used.
function styledWords(text, o, s, paras, hl) {
  const out = [], pre = n => s.bullets === 'none' || !text ? '' : html`<span data-pre="" contenteditable="false">${s.bullets === 'numbers' ? `${n + 1}. ` : '• '}</span>`;
  let line = 0; out.push(pre(0));
  for (const r of runsOf(text, o.spans, hl ? [hl.from, hl.to] : [])) {
    const parts = text.slice(r.from, r.to).split('\n'), on = hl && r.from >= hl.from && r.to <= hl.to;
    for (let i = 0; i < parts.length; i++) {
      if (i) { line++; if (paras && line >= paras) return out; out.push(BR, pre(line)); }
      if (parts[i]) out.push(html`<span class="${on ? 'tsel' : ''}" data-look="${JSON.stringify(r.look)}" style="${runCSS(r.look, s)}">${parts[i]}</span>`);
    }
  }
  return out;
}
function objHTML(o, ctx, ed) {
  if (o.hidden && !ed) return '';
  const f = o.frame, picked = ed && state.sels.includes(o.id), solo = picked && state.sels.length === 1;
  const cls = `ob ${o.kind}${picked ? ' picked' : ''}${o.hidden ? ' ghost' : ''}${o.locked ? ' locked' : ''}${o.fromTheme ? ' fromtheme' : ''}`;
  const flip = (o.flipH || o.flipV || o.rotation) ? `transform:rotate(${o.rotation || 0}deg) scale(${o.flipH ? -1 : 1},${o.flipV ? -1 : 1});` : '';
  const place = `left:${f.x / 19.2}%;top:${f.y / 10.8}%;width:${f.w / 19.2}%;height:${f.h / 10.8}%;opacity:${o.opacity == null ? 1 : o.opacity};${flip}`;
  const handles = solo && !o.locked ? ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'].map(h => html`<i class="hd h-${h}" data-hd="${h}"></i>`) : '';
  const at = { 'data-ob': ed && !o.fromTheme ? o.id : null };
  const badge = ed && o.build && (o.build.in !== 'None' || o.build.out !== 'None') ? html`<i class="ed-badge">BUILD</i>` : '';
  const border = o.stroke && o.stroke.w ? `border:${o.stroke.w / 19.2}cqw solid ${o.stroke.color};` : '';
  const shadow = o.shadow ? 'filter:drop-shadow(0 .4cqw .8cqw rgba(0,0,0,.6));' : '';
  let bd = {}, paras = 0;
  if (ctx.play && o.build) {      // builds are played where the show is drawn
    const b = o.build, kind = (b.in || 'None').toLowerCase().replace(/ /g, '-'), from = (b.from || 'Left').toLowerCase(), steps = ctx.play.steps;
    const mine = steps.map((x, i) => x.id === o.id ? i : -1).filter(i => i >= 0), ins = mine.filter(i => steps[i].phase === 'in'), gone = mine.find(i => steps[i].phase === 'out');
    if (gone != null && gone < ctx.play.done) return '';
    if (ins.length) { const shown = ins.filter(i => i < ctx.play.done).length; if (!shown) return ''; if (b.byPara && o.kind === 'text') paras = shown; bd = { 'data-bdstep': ins[shown - 1], 'data-bdk': `${kind},${from}` }; }
    else if (b.in && b.in !== 'None') bd = { 'data-bd': `${kind},${from},${b.delay || 0}` };
  }
  if (o.kind === 'text') {
    const s = o.style, hl = ed && state.tsel && state.tsel.id === o.id && typeof wordsPicked === 'function' ? wordsPicked() : null;
    let text = o.linked ? linkedValue(o, ctx) : o.text || '';
    const pieces = !o.linked && ((o.spans && o.spans.length) || hl || (ed && s.bullets !== 'none'));      // words styled in the box, or being styled
    if (!pieces) {
      if (paras) text = text.split('\n').slice(0, paras).join('\n');
      if (s.bullets !== 'none' && text) text = text.split('\n').map((l, n) => (s.bullets === 'numbers' ? `${n + 1}. ` : '• ') + l).join('\n');
    }
    const look = `font-family:${FONTS[s.font] || FONTS[DEF_FONT]};font-size:${s.size / 19.2}cqw;font-weight:${s.bold ? 700 : 400};font-style:${s.italic ? 'italic' : 'normal'};text-decoration:${pieces ? 'none' : decoCSS(s)};`
      + `color:${s.color};text-align:${s.align};justify-content:${{ top: 'flex-start', middle: 'center', bottom: 'flex-end' }[s.valign]};letter-spacing:${s.spacing / 19.2}cqw;line-height:${s.lineHeight};`
      + `text-transform:${{ none: 'none', upper: 'uppercase', lower: 'lowercase', title: 'capitalize' }[s.caps]};`
      + (s.shadow ? `text-shadow:${(Math.cos(s.shadowAngle * Math.PI / 180) * s.shadowOffset / 19.2).toFixed(3)}cqw ${(-Math.sin(s.shadowAngle * Math.PI / 180) * s.shadowOffset / 19.2).toFixed(3)}cqw ${s.shadowBlur / 19.2}cqw rgba(0,0,0,${s.shadowOpacity});` : '') + (s.margin ? `padding:${s.margin / 19.2}cqw;` : '') + (s.stroke ? `-webkit-text-stroke:${2 * s.strokeWidth / 19.2}cqw #000;paint-order:stroke fill;` : '')      // the app strokes twice the outline's width under the fill, so the outline shows its width outside the letters
      + ((o.fill || o.gradient) ? `background:${fillCSS(o)};` : '') + border;
    const words = pieces ? styledWords(text, o, s, paras, hl) : lines(text);
    const inner = s.bg ? html`<span class="tx-bg" style="background:${s.bg}">${words}</span>` : words;
    const fit = { 'data-fit': s.scroll === 'none' && /up|down/i.test(s.scaling) ? `${s.size / 19.2},${s.scaling.toLowerCase()}` : null };
    return html`<div class="${cls}${o.linked ? ' linked' : ''}${s.scroll !== 'none' ? ' ticker sc-' + s.scroll : ''}" ${at} ${bd} ${fit} style="${place}${look}"><div class="tx-in" ${{ 'data-tx': ed && !o.linked ? o.id : null }}>${inner}</div>${badge}${handles}</div>`;
  }
  let look;
  if (o.kind === 'image' || o.kind === 'video') { const m = mediaById(o.media); look = `background:${m ? m.bg : '#222'};`; }
  else if (o.kind === 'screen') { const scr = o.source === 'main' ? null : byId(DOC.screens, o.source);      // a live picture of an audience screen, on a stage layout
    return html`<div class="${cls} ob-screen" ${at} ${bd} style="${place}${shadow}"><div class="ob-scr">${screenHTML(scr)}</div><span class="ob-tag">⧉ ${scr ? scr.name : 'Audience screens'}</span>${badge}${handles}</div>`; }
  else if (o.kind === 'web') look = 'background:repeating-linear-gradient(45deg, rgba(255,255,255,.05) 0 6%, transparent 6% 12%);border:.15cqw dashed rgba(255,255,255,.4);';
  else if (o.kind === 'mirror') { const dev = mirrorSet().on && state.mirrorDevice;      // a device shows only where a box is, while its slide is live; it never puts itself on a screen
    if (!dev && !ed) return '';
    return html`<div class="${cls} ob-ndi ob-mirror ${dev ? '' : 'empty'}" ${at} ${bd} style="${place}${shadow}"><span class="ob-tag">📱 ${dev ? state.mirrorDevice : 'Phone or iPad Screen'}</span>${badge}${handles}</div>`; }
  else if (o.kind === 'ndi') { const src = ndiSource(o.source), here = !!o.source && ndiHere(o.source);      // a picture coming in over NDI, as a web page comes in
    if (!here && !ed) return '';      // on the screens (after the app holds the last picture two seconds) and in the lists, a source that isn't sending isn't there
    const state_ = !o.source ? 'empty' : here ? '' : 'gone';      // no source chosen yet: a plain dashed box, as an empty web page's; one not sending: amber
    return html`<div class="${cls} ob-ndi ${state_}" ${at} ${bd} style="${place}${shadow}"><span class="ob-tag">NDI · ${!o.source ? 'choose a source' : src ? src.name : o.source}${!o.source || here ? '' : ndiState() === 'none' ? ' · NDI isn’t installed' : ' · not sending'}</span>${badge}${handles}</div>`; }
  else look = `background:${fillCSS(o)};${CLIP[o.shape] ? `clip-path:${CLIP[o.shape]};` : `border-radius:${o.shape === 'ellipse' ? '50%' : o.shape === 'rounded' ? '3cqw' : '0'};${border}`}`;
  const mark = o.kind === 'web' ? html`<span class="ob-tag">🌐 ${o.url || ''}</span>` : o.kind === 'video' ? html`<span class="ob-tag">▶ VIDEO</span>` : '';
  return html`<div class="${cls}" ${at} ${bd} style="${place}${look}${shadow}">${mark}${badge}${handles}</div>`;
}
// One slide as HTML: the theme's colour, the slide's own media, then its objects.
function slideHTML(slide, ctx = {}, ed = false) {
  if (slide.pending && !ed) return html`<div class="bg" style="background:#111"></div><div class="sl-words">${slide.label || 'A page'}…</div>`;      // a PDF's page still being drawn
  if (slide.img && !ed) return html`<div class="bg" style="background:#000"></div><div class="sl-words">${slideWords(slide)}</div><img class="sl-img" src="${slide.img}" alt="" draggable="false">`;      // the words show if the picture can't be had      // the app's own picture of it (Remote Control)
  const r = resolved(slide, ctx), m = slide.media && !ctx.noMedia ? mediaById(slide.media) : null;
  return html`${ctx.noBg ? '' : html`<div class="bg" style="background:${r.picture ? (mediaById(r.picture) || {}).bg || r.bg : r.bg}"></div>`}${m ? html`<div class="bg" style="background:${m.bg}"></div>` : ''}${r.objects.map(o => objHTML(o, { ...ctx, slide }, ed))}`;
}
const thumb = (slide, ctx) => html`<div class="thumb">${slide ? slideHTML(slide, ctx) : ''}</div>`;

/* ───────── Bible text and passages ───────── */
const bibleText = (bible, book, ch, v) => ((BIBLE_DATA.text[bible] || {})[book] || {})[ch] ? BIBLE_DATA.text[bible][book][ch][v] || '' : '';
const chapterVerses = (book, ch) => Object.keys((BIBLE_DATA.text.kjv[book] || {})[ch] || {}).map(Number);
function refTitle(p, bible) {
  const ru = bible && BIBLE_DATA.bibles[bible] && BIBLE_DATA.bibles[bible].lang === 'ru';
  const name = ru ? BOOK_NAMES_RU[p.book] || p.book : p.book, ch = ru && p.book === 'Psalm' ? p.chapter - 1 : p.chapter;
  return `${name} ${ch}:${p.from}${p.to && p.to !== p.from ? '-' + p.to : ''}`;
}
function passageSlides(p) {
  const b = state.bible, rows = b.rows.filter(r => r.bible !== 'none'), o = b.opts;
  if (!rows.length) return [];
  const vs = chapterVerses(p.book, p.chapter).filter(v => v >= p.from && v <= (p.to || p.from));
  const one = rows.length === 1;
  const make = (key, group, verses, refFor) => ({ id: `b-${p.book}-${p.chapter}-${key}`, group, label: '', notes: '', enabled: true, actions: [], media: null, advance: 0, bible: true, v: verses[0],
    objects: rows.flatMap((r, n) => {
      const name = o.name ? ` · ${BIBLE_DATA.bibles[r.bible].short}` : '';
      const text = verses.map(v => (o.numbers || verses.length > 1 ? v + ' ' : '') + bibleText(r.bible, p.book, p.chapter, v)).join(' ');
      const small = verses.length > 2 ? { size: 30 } : null;
      const t = { id: `${key}-${r.verse}`, kind: 'text', name: `Verse ${n + 1}`, role: r.verse, text, opacity: 1, ...(one ? { frame: { x: 130, y: 130, w: 1660, h: 660 }, style: { size: verses.length > 2 ? 38 : 60 } } : small ? { style: small } : {}) };
      const ref = refFor ? { id: `${key}-${r.ref}`, kind: 'text', name: `Reference ${n + 1}`, role: r.ref, text: refTitle(refFor, r.bible) + name, opacity: 1, ...(one ? { frame: { x: 130, y: 820, w: 1660, h: 100 }, style: { size: 46 } } : {}) } : null;
      return ref && r.ref !== 'none' ? [t, ref] : [t];
    }) });
  if (o.per === 'passage') return vs.length ? [make(`${p.from}-${p.to}`, refTitle(p), vs, o.refAt === 'none' ? null : p)] : [];
  return vs.map((v, n) => make(String(v), `${p.book} ${p.chapter}:${v}`, [v],
    o.refAt === 'every' ? { ...p, from: v, to: v } : o.refAt === 'passage-every' ? p : o.refAt === 'last' ? (n === vs.length - 1 ? p : null) : null));
}

/* ───────── decks: what the middle column lists, each a song, a passage or one tile ───────── */
function seqOf(song) {
  const on = song.slides.map((s, i) => s.enabled ? i : -1).filter(i => i >= 0);
  const a = song.arrangement && byId(song.arrangements, song.arrangement);
  return a ? a.groups.flatMap(g => on.filter(i => song.slides[i].group === g)) : on;
}
const songDeck = (s, key, item) => ({ key, kind: 'song', title: s.title, song: s, slides: s.slides, seq: seqOf(s), ctx: { song: s, kind: 'songs', version: item && item.version }, item });      // an item's tiles are drawn in the version it shows with
function itemDeck(it) {
  if (it.kind === 'song' || (it.kind === 'placeholder' && it.song)) { const s = songById(it.song); return s ? { ...songDeck(s, it.id, it), slot: it.kind === 'placeholder' ? it.name : null } : { key: it.id, kind: 'missing', title: it.title || 'A song that can’t be found', item: it }; }
  if (it.kind === 'passage') { const sl = passageSlides(it.passage); return { key: it.id, kind: 'passage', title: refTitle(it.passage), slides: sl, seq: sl.map((_, i) => i), ctx: { kind: 'bible', theme: state.bible.opts.theme, version: it.version }, item: it }; }
  if (it.kind === 'header') return { key: it.id, kind: 'header', title: it.name, item: it };
  if (it.kind === 'app') { const sl = wordsSlide(it.name, it.name); sl.id = 'app-' + it.id; return { key: it.id, kind: 'passage', app: true, title: it.name, slides: [sl], seq: [0], ctx: { raw: true, bg: '#000000' }, item: it }; }      // something only the real app can open, such as a saved Bible passage
  if (it.kind === 'media') { const m = mediaById(it.media); return { key: it.id, kind: 'media', title: m ? mediaLabel(m) : 'A picture or video that has gone', media: m, item: it }; }
  if (it.kind === 'mediaPlaylist') { const mp = byId(DOC.mediaPlaylists, it.mp); return { key: it.id, kind: 'mediaPlaylist', title: mp ? mp.name : 'A media playlist that has gone', mp, item: it }; }
  return { key: it.id, kind: 'placeholder', title: it.name, item: it };
}
function voxDeck() {
  const slides = DOC.voxlive.screens.map(v => ({ id: 'vox-' + v.id, group: v.name, label: `${v.lang} · ${v.mode}`, notes: '', enabled: true, actions: [], media: null, vox: v.id,
    objects: [{ id: 'voxt-' + v.id, kind: 'text', name: 'Test pattern', text: `VoxLive\n${v.name}`, frame: { x: 160, y: 300, w: 1600, h: 480 }, style: { size: 90, color: '#7ad7ff', shadow: false }, opacity: 1 }] }));
  return { key: 'vox', kind: 'vox', title: 'VoxLive', slides, seq: slides.map((_, i) => i), ctx: { raw: true, bg: '#04121c' } };
}
function bibleDeck() {
  const r = state.bible.ref, per = state.bible.opts.per === 'verse', vs = chapterVerses(r.book, r.chapter);
  const sl = passageSlides(per ? { ...r, from: vs[0], to: vs[vs.length - 1] } : r);
  return { key: 'bible', kind: 'passage', title: `${r.book} ${r.chapter}`, slides: sl, seq: sl.map((_, i) => i), ctx: { kind: 'bible', theme: state.bible.opts.theme } };
}
let deckCache = null;
function decks() {
  if (deckCache) return deckCache;
  if (state.src.type === 'lib') { const s = songById(state.openSong); return deckCache = s ? [songDeck(s, 'lib-' + s.id)] : []; }
  if (state.src.type === 'vox') return deckCache = [voxDeck()];
  if (state.src.type === 'cat') { const s = catSongs().find(x => x.id === state.openCat) || catSongs()[0]; if (s) state.openCat = s.id; return deckCache = s ? [songDeck(s, 'cat-' + s.id)] : []; }      // a catalog song, to look at
  return deckCache = playlist().items.map(itemDeck);
}
const deckByKey = key => key === 'bible' ? bibleDeck() : key === 'bible-held' ? state.heldBible : key === 'vox' ? voxDeck() : decks().find(d => d.key === key) || (state.heldDeck && state.heldDeck.key === key ? state.heldDeck : undefined);      // heldDeck: the song on screen after its item left the playlist
const liveSlideIndex = () => { const d = state.live && deckByKey(state.live.deck); return d && d.seq ? (state.live.q >= 0 ? d.seq[state.live.q] : state.live.s) : -1; };
const isLive = (key, s) => !!state.live && !!state.on && state.live.deck === key && liveSlideIndex() === s;

/* ───────── the show ───────── */
Object.defineProperty(state, 'layers_slide', { get: () => !!state.on });
const layerOn = k => ({ slide: !!state.on, media: !!state.mediaOn, props: state.propsOn.length > 0, messages: !!state.message, audio: state.audio.playing, announcements: !!state.ann })[k];
const anythingOn = () => ['slide', 'media', 'props', 'messages', 'audio', 'announcements'].some(layerOn);
const videoOn = () => { const m = state.mediaOn && mediaById(state.mediaOn); return m && m.video ? m : null; };
// The announcements layer plays a song's slides in turn, or a picture or video from the media strip (as built, 24 November).
// Clicking what plays there (or last played there) again plays it there again, never as media.
const annMedia = () => state.ann && state.ann.media ? mediaById(state.ann.media) : null;
const annName = () => { const a = state.ann, s = a && a.song && songById(a.song), m = annMedia(); return s ? s.title : m ? mediaLabel(m) : ''; };
function playAnnMedia(id) { const m = mediaById(id); if (!m) return; state.ann = { media: id }; state.annLast = id; state.annOffSlides = false;
  state.annVideo = { pos: 0, playing: !!m.video, loop: !!m.loop, mute: false }; render(); }
// The one video row under the preview acts on the media video, or else on a video announcement
const rowVideo = () => { const m = videoOn(); if (m) return { m, v: state.video, key: 'video' }; const a = annMedia(); return a && a.video && state.annVideo ? { m: a, v: state.annVideo, key: 'annVideo' } : null; };
const lenText = t => t >= 3600 ? `${Math.floor(t / 3600)}:${two(Math.floor(t / 60) % 60)}:${two(Math.floor(t % 60))}` : mmss(t);      // a video's length on its tile
const transitionFor = (slide, song) => (slide && slide.transition) || (song && song.transition) || DOC.settings.transition;

function putUp() {
  const d = deckByKey(state.live.deck); if (!d || !d.slides) return;
  const s = liveSlideIndex(), slide = d.slides[s]; if (!slide) return;
  state.on = { slide: clone(slide), songId: d.song ? d.song.id : null, kind: d.ctx.kind, raw: d.ctx.raw, bg: d.ctx.bg, theme: d.ctx.theme, title: d.title, group: slide.group, n: state.live.q >= 0 ? state.live.q + 1 : s + 1, total: d.seq.length,
    tr: transitionFor(slide, d.song), stamp: Date.now() };
}
const screenIsStale = () => { if (!state.live || !state.on) return false; const d = deckByKey(state.live.deck), sl = d && d.slides && d.slides[liveSlideIndex()]; return !!sl && JSON.stringify(sl) !== JSON.stringify(state.on.slide); };

function go(key, q, s) {
  if (state.heldDeck && key !== state.heldDeck.key) state.heldDeck = null;
  // Theme versions: one switched on before anything is up waits for the next item; otherwise the next item ends it unless it
  // is kept; an item set to show with a version brings it, until the item after it.
  if (state.version) { if (state.version.from == null) state.version.from = key; else if (!state.version.keep && key !== state.version.from) state.version = null; }
  { const it = (deckByKey(key) || {}).item; if (it && it.version && byId(DOC.versions, it.version) && !(state.version && state.version.from === key)) state.version = { id: it.version, from: key, keep: !!it.versionKeep, item: true }; }
  if (state.ann) state.annOffSlides = true;      // the owner, 22 November: a slide going up takes the announcements off every screen that shows slides, until they are started again
  const d = deckByKey(key); if (!d || !d.slides || !d.slides.length) return;
  if (key !== 'bible-held') state.heldBible = null;      // the cue stays while what is on screen runs on; Enter takes it
  if (q == null) q = d.seq.indexOf(s);             // a slide switched off, or left out of the arrangement, can still be clicked
  state.live = { deck: key, q, s: q >= 0 ? d.seq[q] : s };
  if (state.cue && state.cue.deck === key && state.cue.q === q) state.cue = null;      // running into the cued slide uses it up
  state.vox = null; state.build = 0; putUp();
  const slide = d.slides[state.live.s];
  if (slide.vox) state.vox = slide.vox;
  if (slide.media) showMedia(slide.media, true);
  state.advanceLeft = slide.advance || 0;
  (slide.actions || []).forEach(runStep);
  if (key === 'bible') noteHistory();
  else if (key !== 'vox') state.openItem = key;
  render();
  followLive();
}
// Keep the slide that is on screen in view: clear of the title bar stuck at the top and of the fade above the shelf at the bottom.
// The readout under the preview takes the slides to the one on screen, opening its item (it was "Go to the Slide on Screen" in the Mac menu)
A.goLiveSlide = () => { const l = state.live; if (!l) return; if (state.mode !== 'show') state.mode = 'show'; if (decks().some(d => d.key === l.deck)) state.openItem = l.deck; render(); requestAnimationFrame(() => followLive('.live')); };
function followLive(sel = '.live') {
  const el = document.querySelector(`#slides ${sel}[data-a^="go"]`) || document.querySelector(`${sel}[data-a^="go"]`), how = 'auto';      // at once: an animated scroll can be cut short by the next key press and leave the slide out of view
  if (el && el.closest('#slides')) {
    const box = $('slides'), r = el.getBoundingClientRect(), b = box.getBoundingClientRect(), top = 60, bottom = 44;
    if (r.top < b.top + top) box.scrollTo({ top: box.scrollTop + r.top - b.top - top, behavior: how });
    else if (r.bottom > b.bottom - bottom) box.scrollTo({ top: box.scrollTop + r.bottom - b.bottom + bottom, behavior: how });
  } else if (el) el.scrollIntoView({ block: 'nearest', behavior: how });
  const row = document.querySelector('#left .item.live'); if (row) row.scrollIntoView({ block: 'nearest', behavior: how });      // and its row in the list on the left
}
// The item that is open on the left is where the keys are aimed. When it is not the one on screen (Up, Down or a click moved it),
// Next or Previous starts it from its first slide instead of carrying on in the song that is showing.
function aimed() {
  if (state.mode !== 'show' || !state.live) return null;
  const all = decks(), open = all.find(x => x.key === (state.src.type === 'lib' ? 'lib-' + state.openSong : state.openItem));
  if (!open || open.key === state.live.deck || !all.some(x => x.key === state.live.deck)) return null;
  return (open.slides && open.seq.length) || (open.kind === 'media' && open.media) || (open.kind === 'mediaPlaylist' && open.mp) ? open : null;
}
// The cue: a slide or verse picked by the keyboard (a search, or Up and Down), ready to go on screen. It is never orange:
// nothing is on screen until Enter, Space or Right puts it there. Before then the arrows move it; afterwards they move what is live.
function setCue(key, q, s) {
  const d = deckByKey(key); if (!d || !d.slides || !d.seq.length) { state.cue = null; return; }
  if (q == null || q < 0) q = d.seq.indexOf(s); if (q < 0) q = 0; state.cue = { deck: key, q, s: d.seq[q] };
}
const cueIsLive = () => !!state.cue && !!state.live && !!state.on && state.live.deck === state.cue.deck && state.live.q === state.cue.q;
// a cue belongs to the view it was made in: a verse in Bible, a slide in Service
const cueHere = () => !!state.cue && !cueIsLive() && (state.mode === 'bible' ? state.cue.deck === 'bible' : state.mode === 'show' && state.cue.deck !== 'bible');
const isCued = (key, q, s) => !!state.cue && !cueIsLive() && state.cue.deck === key && (q >= 0 ? state.cue.q === q : state.cue.s === s);
function moveCue(dir) {
  const c = state.cue, d = c && deckByKey(c.deck); if (!d) return;
  const q = c.q + dir; if (q < 0 || q >= d.seq.length) return;      // at either end nothing moves and nothing is said (the owner: those notices are not needed)
  state.cue = { deck: c.deck, q, s: d.seq[q] }; render(); followLive('.cued');
}
function goCue() { const c = state.cue; state.cue = null; go(c.deck, c.q, c.s); }
// Something opened by the keyboard is cued at its first slide, unless it is the one on screen, which simply carries on.
function cueFirst(key) { const d = deckByKey(key); if (!d || !d.slides || !d.seq.length || (state.live && state.on && state.live.deck === key)) { state.cue = null; return; } setCue(key, 0); }
function step(dir) {
  // With nothing on screen, Space puts the cue on (the arrows only move it: see the key handler). With something on screen,
  // Space and the arrows keep running that, so a song can be finished; Enter takes the cue.
  if (cueHere()) { if (!state.on) return dir > 0 ? goCue() : moveCue(-1); }
  const aim = cueHere() ? null : aimed();
  if (aim) return aim.slides ? go(aim.key, 0) : aim.kind === 'media' ? showMedia(aim.media.id) : playMediaPlaylist(aim.mp.id);
  let d = state.live && deckByKey(state.live.deck);
  if (!d || !d.slides) {
    if (state.mode === 'bible') { const bd = bibleDeck(), at = state.bible.opts.per === 'verse' ? Math.max(0, bd.slides.findIndex(sl => sl.v === state.bible.ref.from)) : 0; return go('bible', at); }
    const start = decks().find(x => x.key === (state.src.type === 'lib' ? 'lib-' + state.openSong : state.openItem) && x.slides) || decks().find(x => x.slides && x.slides.length);
    return start && go(start.key, 0);
  }
  if (state.on && !state.vox) {      // Next sets a slide's click builds going in order before it moves on; Previous takes them back
    const total = buildSteps(state.on.slide).length;
    if (dir > 0 && state.build < total) { state.build++; return renderMonitor(); }
    if (dir < 0 && state.build > 0) { state.build--; return renderMonitor(); }
  }
  const q = (state.live.q >= 0 ? state.live.q : d.seq.findIndex(i => i > state.live.s) - (dir > 0 ? 1 : 0)) + dir;
  if (q >= 0 && q < d.seq.length) return go(d.key, q);
  if (d.song && d.song.loop && dir > 0) return go(d.key, 0);
  // At an end, Next and Previous stay put and say nothing (the owner: those notices are not needed): the last verse of a passage,
  // a song from elsewhere, a library's song, Continuous off, or either end of the playlist.
  if (d.key.startsWith('bible') || (d === state.heldDeck && !decks().some(x => x.key === d.key)) || d.key === 'vox' || state.src.type !== 'pl' || !DOC.settings.continuous) return;
  const all = decks(), at = all.findIndex(x => x.key === d.key);
  for (let n = at + dir; n >= 0 && n < all.length; n += dir) if (all[n].slides && all[n].seq.length && (all[n].kind === 'song' || all[n].kind === 'passage')) return go(all[n].key, dir > 0 ? 0 : all[n].seq.length - 1);   // on through songs and passages; over media, placeholders and headers
}
function clear(layer) {
  for (const k of layer === 'all' ? ['slide', 'media', 'props', 'messages', 'audio', 'announcements'] : [layer]) {
    if (k === 'slide') { state.on = null; state.vox = null; state.advanceLeft = 0; }
    if (k === 'media') { state.mediaOn = null; state.mediaPl = null; }
    if (k === 'props') state.propsOn = [];
    if (k === 'messages') state.message = null;
    if (k === 'audio') { state.audio.playing = false; if (typeof syncAudio === 'function') syncAudio(); }
    if (k === 'announcements') { state.ann = null; state.annOffSlides = false; }
  }
  render();
}
function showMedia(id, quiet) {
  const m = mediaById(id); if (!m) return;
  state.mediaOn = id; if (!quiet) state.mediaPl = null;
  Object.assign(state.video, { pos: 0, playing: true, loop: !!m.loop });
  if (!quiet) render();
}
function playMediaPlaylist(id) { const mp = byId(DOC.mediaPlaylists, id); if (!mp || !mp.items.length) return refuse('That playlist is empty'); state.mediaPl = { id, n: 0, left: 8 }; showMedia(mp.items[0], true); render(); }

// What a macro step or a slide's action does.
function runStep(st) {
  if (st.do === 'clear') { const keep = deckCache; clear(st.what); deckCache = keep; }
  if (st.do === 'audience') state.audience = st.on;
  if (st.do === 'timer') { const t = DOC.timers.find(x => x.name === st.name); if (t) { if (st.what === 'reset') { t.left = t.start || 0; t.run = false; } else t.run = st.what === 'start'; } }
  if (st.do === 'message') { if (st.what === 'hide') state.message = null; else { const m = DOC.messages.find(x => x.name === st.name); if (m) showMessage(m.id, true); } }
  if (st.do === 'prop') { const p = DOC.props.find(x => x.name === st.name); if (p) { state.propsOn = state.propsOn.filter(x => x !== p.id); if (st.what === 'show') state.propsOn.push(p.id); } }
  if (st.do === 'media') showMedia(st.id, true);
  if (st.do === 'ann') { const s = songById(st.id); if (s) { state.ann = { song: s.id, q: 0, left: s.slides[0].advance || 8 }; state.annOffSlides = false; } }
  if (st.do === 'version') versionUse(st.id && byId(DOC.versions, st.id) ? st.id : null, st.keep, true);
  if (st.do === 'song') { const d = decks().find(x => x.song && x.song.id === st.id); if (d) { state.openItem = d.key; } }
}
// A step whose song, picture, message, prop, timer or version has gone reads in amber and says so
const stepGone = st => ({ song: () => !songById(st.id), ann: () => !songById(st.id), media: () => !mediaById(st.id), message: () => st.what !== 'hide' && !DOC.messages.some(x => x.name === st.name),
  prop: () => !DOC.props.some(x => x.name === st.name), timer: () => !DOC.timers.some(x => x.name === st.name), version: () => !!st.id && !byId(DOC.versions || [], st.id) })[st.do]?.() || false;
const stepText = st => ({ clear: `Clear ${st.what === 'all' ? 'everything' : 'the ' + st.what}`, audience: `Audience screens ${st.on ? 'on' : 'off'}`, timer: `${{ start: 'Start', stop: 'Stop', reset: 'Reset' }[st.what]} timer “${st.name}”`,
  message: st.what === 'hide' ? 'Hide the message' : `Show message “${st.name}”`, prop: `${st.what === 'show' ? 'Show' : 'Hide'} prop “${st.name}”`, media: `Show ${(mediaById(st.id) || { name: 'a picture' }).name}`, song: songById(st.id) ? `Go to “${songById(st.id).title}”` : 'Go to: (a song no longer in the library)', ann: songById(st.id) ? `Play “${songById(st.id).title}” on the announcements layer` : 'Play on the announcements layer: (a song no longer in the library)', version: st.id ? `Use the ${versionName(st.id) || 'gone'} version${st.keep ? ' until switched back' : ''}` : 'Back to the usual look' })[st.do];
function showMessage(id, quiet) { const m = byId(DOC.messages, id); state.message = id; state.messageLeft = m.leaves || 0; if (!quiet) render(); }
const messageText = m => m.text.replace(/\{clock\}/g, new Date().toLocaleTimeString('en-GB').slice(0, 5)).replace(/\{timer:([^}]+)\}/g, (_, n) => { const t = DOC.timers.find(x => x.name === n); return t ? fmt(t.left).replace(/^00:/, '') : '--:--'; });

// What a screen shows: its layers, in order, in its own theme.
function screenHTML(scr, inner) {
  if (scr && scr.keyOf && scr.kind === 'Audience' && !inner) { const f = byId(DOC.screens, scr.keyOf);      // a key: the fill's shape, white on black
    if (!f || f.kind !== 'Audience' || f.keyOf) return html`<div class="bg" style="background:#000"></div>`;
    if (f.bgMode !== 'clear') return html`<div class="bg" style="background:#fff"></div>`;
    return html`<div class="bg" style="background:#000"></div><div class="lv-keyed">${screenHTML(f, true)}</div>`; }
  const has = k => !scr || !scr.layers || scr.layers.includes(k), clear = scr && scr.bgMode === 'clear';      // see-through: no background, no media
  const theme = scr && scr.theme, on = state.on, m = !clear && state.mediaOn && mediaById(state.mediaOn);
  const h = [clear ? (inner ? '' : html`<div class="bg lv-clear"></div>`) : html`<div class="bg" style="background:${(scr && scr.colour) || '#000'}"></div>`];
  if (state.test !== 'off') return html`${h}<div class="bg lv-test ${state.test}"></div>`;
  if (m && has('media') && !(m.url && m.video && !scr)) h.push(html`<div class="bg" style="background:${m.bg}"></div>`);
  if (on && has('slide')) {
    const song = on.songId ? songById(on.songId) : null, ctx = { song, kind: on.kind, raw: on.raw, bg: on.bg, theme: theme || on.theme, noBg: true, noMedia: true, version: state.version && state.version.id };
    ctx.play = { steps: buildSteps(on.slide), done: state.build };
    h.push(html`<div class="lv-slide">${state.vox ? html`<div class="lv-vox"><b>${(byId(DOC.voxlive.screens, state.vox) || {}).name || ''}</b><span>live captions appear here</span></div>` : slideHTML(on.slide, ctx)}</div>`);
    const where = DOC.settings.copyrightAt, cr = song && (where === 'every' || (where === 'first' && on.n === 1) || (where === 'last' && on.n === on.total)) ? copyrightLine(song) : '';
    if (cr && !state.vox) h.push(html`<div class="lv-copy">${cr}</div>`);
  }
  if (state.ann && has('announcements') && !(state.annOffSlides && has('slide'))) { const s = state.ann.song && songById(state.ann.song), am = annMedia();
    if (s) h.push(html`<div class="lv-ann">${slideHTML(s.slides[state.ann.q], { song: s, kind: 'songs', theme, noMedia: true })}</div>`);
    else if (am) h.push(html`<div class="lv-ann lv-annmedia" style="background:${am.bg}">${am.poster ? html`<img class="sl-img" src="${am.poster}" alt="">` : ''}</div>`); }
  if (has('props')) for (const id of state.propsOn) { const p = byId(DOC.props, id); if (p) h.push(slideHTML({ objects: p.objects }, { raw: true, noBg: true })); }
  if (state.message && has('messages')) { const msg = byId(DOC.messages, state.message); if (msg) h.push(html`<div class="lv-msg ${msg.look.toLowerCase().replace(' ', '-')}">${messageText(msg)}</div>`); }
  const E = scr && Array.isArray(scr.edges) ? scr.edges : null;      // covered edges, in the slide's pixels (1920 wide = 100cqw)
  if (E && E.some(Number)) h.push(html`<div class="lv-edges" style="border-width:${E[1] / 19.2}cqw ${E[2] / 19.2}cqw ${E[3] / 19.2}cqw ${E[0] / 19.2}cqw;border-color:${clear ? 'transparent' : scr.colour || '#000'}"></div>`);
  if (state.identify && scr) h.push(html`<div class="lv-ident" style="--c:${scr.tint}"><b>${scr.name}</b></div>`);
  return html`${h}`;
}
// What a stage layout's boxes need to know.
function stageCtx() {
  const on = state.on, d = state.live && deckByKey(state.live.deck), nq = state.live ? state.live.q + 1 : 0;
  const next = d && d.slides && d.seq[nq] != null ? slideWords(d.slides[d.seq[nq]]) : '';
  return { raw: true, bg: '#000000', currentWords: on ? allWords(on.slide) : '', nextWords: next, notes: on ? on.slide.notes : '', title: on ? on.title : '', slide: on ? on.slide : null, song: on && on.songId ? songById(on.songId) : null };
}
const stageHTML = layoutId => { const l = byId(DOC.stageLayouts, layoutId) || DOC.stageLayouts[0]; return html`${slideHTML({ objects: l.objects }, stageCtx())}${state.stageMsg ? html`<div class="lv-msg band">${state.stageMsg}</div>` : ''}`; };

// Bring an item to the top of the slides. Its bar sticks to the top once scrolled past, so the browser thinks a stuck bar is
// already in view and won't move; a marker just before each bar, which never sticks, says where the item really starts.
function scrollToDeck(id, smooth) {
  const box = $('slides'), mark = $('at-' + id); if (!mark) return false;
  const top = mark.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop;
  box.scrollTo({ top, behavior: smooth === false || state.smooth === false ? 'auto' : 'smooth' });
  return true;
}

/* ───────── render: each mode fills the left and the middle; the right column is always the same ───────── */
const VIEWS = {};
let drawing = false, lastMode = null, pointerHeld = false, pointerEl = null, fromField = false, renderWaiting = false, renderTimer = 0;
function flushRender() { if (renderWaiting) { renderWaiting = false; render(); } }
function render() {
  deckCache = null;
  if (typeof dragging !== 'undefined' && dragging) { renderWaiting = true; return; }      // a redraw would throw away the thing being dragged and end the drag; it waits for the drop
  if (fromField && pointerHeld) { renderWaiting = true; clearTimeout(renderTimer); renderTimer = setTimeout(flushRender, 250); return; }      // the button under the pointer must survive until its click
  drawing = true;      // a box being typed in, taken away by the redraw, loses its focus: that is not someone leaving it
  try { renderTop(); (VIEWS[state.mode] || VIEWS.show).left(); (VIEWS[state.mode] || VIEWS.show).centre(); renderRight(); } finally { drawing = false; }
  if (lastMode !== state.mode) { $('slides').scrollTop = 0; $('left').scrollTop = 0; lastMode = state.mode; }
  fitAll(document.body);
  if (state.retype) { const r = state.retype; state.retype = null; startTyping(r.id, r.from, r.to); }      // typing goes on where it was, after a style was set from the keyboard or a button
}
function renderTop() {
  $('brand').textContent = BRAND; put('navToggle', I.list);
  // the owner named the first view Service (23 November; the app e3d08de); inside it is still 'show'
  put('modes', [['show', 'Service', 'S'], ['bible', 'Bible', 'B'], ['edit', 'Edit', 'E']].map(([m, name, k]) => html`<button class="${state.mode === m || (m === 'edit' && state.mode === 'themes') ? 'on' : ''}" data-a="mode:${m}" title="${name} (${keyText('⌥' + k)})">${name}<kbd class="kh">${keyText('⌥' + k)}</kbd></button>`));
  $('gear').classList.toggle('on', state.mode === 'settings');
  document.body.classList.toggle('show-keys', !!DOC.settings.showKeys);      // key tags: a setting, not a held key
  document.body.classList.toggle('no-left', state.mode === 'bible' && !state.bible.lists);
  for (const [id, key] of [['outAudience', 'audience'], ['outStage', 'stage']]) { $(id).classList.toggle('on', state[key]); $(id).querySelector('b').textContent = state[key] ? 'ON' : 'OFF'; }
  renderNote();      // a song changed outside waits in the notice slot for an answer
}
const BRAND = 'SoftPresenter';
A.mode = ([m]) => { if (m !== state.mode) state.cue = null;
  if (m === 'settings') m = state.mode === 'settings' ? 'show' : 'settings';
  if (m === 'edit') return enterEdit();
  if (m === 'themes') { state.sels = []; state.reflow = false; }
  state.mode = m; render();
};
A.out = ([k]) => { state[k] = !state[k]; render(); };
A.outside = ([what]) => { if (state.notesOpen) NOTES.forEach(n => { n.seen = true; }); const s = songById(state.changedOutside); state.changedOutside = null; render(); toast(what === 'reload' ? `${s.title} was read again from its file` : `SoftPresenter's version of ${s.title} was written over the file`); };

// Renaming in place (as the app has it, 22 November): the name keeps exactly its look (face, weight, ink, capitals and spacing,
// on the same pixels). The only signs are a 1.5 px inner edge in faint, the selection in sel grey and the caret in the words'
// colour. Return or a click elsewhere keeps it; Escape puts it back. A long name opens scrolled to its end, and a cut-short or
// two-line name shows in full on one line while it is retyped.
function renameInPlace(el, now, done) {
  if (!el || !el.isConnected || !el.offsetParent) return false;
  const drag = el.closest('[draggable="true"]'); if (drag) drag.setAttribute('draggable', 'false');
  el.classList.add('renaming'); el.setAttribute('contenteditable', 'plaintext-only'); el.setAttribute('spellcheck', 'false'); el.textContent = now; el.focus();
  const r = document.createRange(); r.selectNodeContents(el); const sel = getSelection(); sel.removeAllRanges(); sel.addRange(r); el.scrollLeft = el.scrollWidth;
  let over = false;
  const finish = keep => { if (over) return; over = true; const v = el.textContent.replace(/\s+/g, ' ').trim();
    el.removeAttribute('contenteditable'); el.classList.remove('renaming'); if (drag) drag.setAttribute('draggable', 'true'); el.removeEventListener('keydown', key); el.removeEventListener('blur', blur);
    if (keep && v && v !== now) done(v); else { el.textContent = now; render(); } };
  const key = e => { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); finish(true); } else if (e.key === 'Escape') { e.preventDefault(); finish(false); } };
  const blur = () => finish(true);
  el.addEventListener('keydown', key); el.addEventListener('blur', blur);
  return true;
}
// rename where the name is shown when it is on view; otherwise a small sheet
const renameAt = (el, now, done, title, label = 'Name') => { if (!renameInPlace(typeof el === 'string' ? document.querySelector(el) : el, now, done)) ask(title, label, now, done); };

// A media file's label in the media bin and in a playlist (the owner's README pictures, 23 November): its name without the
// extension (the VIDEO badge and the kind's mark say what it is), unless another file in the bin has the same name in another
// format, when both keep their extensions to be told apart. Menus, sheets and tooltips show the whole file name. A label still
// too long for its place ends in … on one line.
const bareName = n => n.replace(/\.[A-Za-z0-9]{1,5}$/, '');
const mediaLabel = m => DOC.media.some(o => o !== m && !o.deck && bareName(o.name).toLowerCase() === bareName(m.name).toLowerCase()) ? m.name : bareName(m.name);

// Keys as the computer names them (the owner, 23 November): the Mac's symbols (⌃ ⌥ ⇧ ⌘) on a Mac, an iPad or an iPhone; words
// elsewhere, in the usual order (Ctrl+Alt+Shift+). Every key is written once with the symbols and shown through keyText, in
// hover hints, menus, the key tags and both lists of keys. Clicks keep their words on every computer ("Shift-click").
// `?keys=pc` or `?keys=mac` shows the other way, for checking; `?os=linux` (or `?os=windows`) shows that system.
const OS_ASKED = new URLSearchParams(location.search).get('os');
const APPLE = OS_ASKED === 'linux' || OS_ASKED === 'windows' ? false : (k => k === 'pc' ? false : k === 'mac' ? true : /mac|iphone|ipad|ipod/i.test((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || navigator.userAgent))(new URLSearchParams(location.search).get('keys'));
// Windows on its own (what Install… of the sp command says after: PowerShell keeps sp for itself; the firewall asks before
// NDI goes out); ?keys=pc counts as Windows. Linux is the rest: no NDI Tools there, so the app downloads NDI itself.
const WINDOWS = !APPLE && OS_ASKED !== 'linux' && (OS_ASKED === 'windows' || new URLSearchParams(location.search).get('keys') === 'pc' || /win/i.test((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || navigator.userAgent));
const LINUX = !APPLE && !WINDOWS;
const keyWords = s => String(s).replace(/[⌃⌥⇧⌘]+/g, m => [(m.includes('⌃') || m.includes('⌘')) && 'Ctrl', m.includes('⌥') && 'Alt', m.includes('⇧') && 'Shift'].filter(Boolean).join('+') + '+');
const keyText = s => APPLE ? String(s) : keyWords(s);

// NDI: what the finder sees ('none' when the NDI runtime isn't installed on this computer), and whether a source is sending now
const ndiState = () => state.ndi || 'found';
const ndiSource = id => NDI_SOURCES.find(n => n.id === id);
const ndiHere = id => ndiState() === 'found' && !!ndiSource(id) && !(state.ndiLost || []).includes(id);

/* ───────── theme versions: one name, every screen that has it ───────── */
const versionName = id => (byId(DOC.versions || [], id) || { name: '' }).name;
const themesWith = id => DOC.themes.filter(t => t.templates.some(tp => tp.version === id));
function versionUse(id, keep, quiet) {
  state.version = id ? { id, from: state.live && state.on ? state.live.deck : null, keep: !!keep } : null;
  if (!quiet) { render(); void (id ? `The ${versionName(id)} version is on${themesWith(id).length < DOC.themes.length ? ` where a theme has it` : ''}` : 'Back to the usual look'); }
}
A.versionUse = ([id, keep]) => versionUse(id || null, keep === 'keep');
// The manual switch (the bar under the slides) and the item's own choice share one menu shape
M.version = () => [{ head: 'Show the screens in' }, { t: 'The usual look', on: !state.version, f: () => versionUse(null) },
  ...DOC.versions.map(v => ({ t: `The ${v.name} version`, on: !!state.version && state.version.id === v.id, f: () => versionUse(v.id, state.version && state.version.keep) })), { sep: true },
  { t: 'Keep It Until Switched Back', on: !!state.version && state.version.keep, dim: !state.version, f: () => { if (state.version) { state.version.keep = !state.version.keep; render(); } } },
  { t: 'Make or Change Versions…', f: () => { A.mode(['edit']); A.edKind(['themes']); } }];
M.itemVersion = ([id]) => { const it = itemById(id); return [{ head: 'When this item goes on screen, show it in' }, { t: 'The usual look', on: !it.version, f: () => mutate(() => { delete it.version; delete it.versionKeep; }) },
  ...DOC.versions.map(v => ({ t: `The ${v.name} version`, on: it.version === v.id, f: () => mutate(() => { it.version = v.id; }) })), { sep: true },
  { t: 'Keep It After This Item', on: !!it.versionKeep, dim: !it.version, f: () => it.version && mutate(() => { it.versionKeep = !it.versionKeep; }) },
  { t: 'It applies in this playlist only; the song itself doesn’t change', dim: true, f: () => {} }]; };      // the item's ⋯ menu takes all but the heading and this last note

/* ═════════ show.js ═════════ */
// Show: the library and playlists on the left, the slides in the middle, the media bin under them.
'use strict';

VIEWS.show = { left: renderShowLeft, centre: renderShowCentre };

/* ───────── left: where the songs come from, then what is in it ───────── */
const srcName = () => state.src.type === 'pl' ? playlist().name : state.src.type === 'vox' ? 'VoxLive' : state.src.type === 'cat' ? CATALOG.name : state.src.id === 'all' ? 'All Songs' : byId(DOC.libraries, state.src.id).name;
const librarySongs = () => state.src.id === 'all' ? DOC.songs.filter(s => !s.notInLibrary) : byId(DOC.libraries, state.src.id).songs.map(songById).filter(Boolean);
const voxReady = () => DOC.voxlive.connected && DOC.voxlive.screens.length > 0;

function renderShowLeft() {
  const f = state.filter.trim().toLowerCase(), hit = t => !f || t.toLowerCase().includes(f);
  const dots = (kind, id) => html`<button class="iconbtn sh-dots" data-a="menu:${kind},${id}" aria-label="More">${I.more}</button>`;
  const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;
  // One row of the chooser: a mark for what kind of thing it is, its name, what is in it, and a tick on the one that is open
  const src = (type, id, name, sub, menu = '') => { const on = state.src.type === type && state.src.id === id;
    return html`<div class="sh-src ${type} ${on ? 'on' : ''}"><button class="rowbtn" data-a="src:${type},${id}"><span class="sh-mark">${type === 'pl' ? I.playlist : type === 'cat' ? I.catalog : I.library}</span><span class="t">${name}</span><span class="meta mono" title="${sub}">${sub.split(' ')[0]}</span>${on ? html`<span class="sh-open">${I.check}</span>` : ''}</button>${menu}</div>`; };
  const panel = html`<div class="sh-panel ${state.srcMenu ? '' : 'closed'}">
    <div class="sect-h"><span class="cap">Playlists</span><span class="sh-what">a service, in order</span><button class="iconbtn" data-a="newPlaylist" title="New playlist">${I.plus}</button></div>
    ${DOC.playlists.map(p => src('pl', p.id, p.name, plural(p.items.filter(i => i.kind !== 'header').length, 'item'), dots('pl', p.id)))}
    <div class="sect-h sh-second"><span class="cap">Song libraries</span><span class="sh-what">every song you keep</span><button class="iconbtn" data-a="newLibrary" title="New library">${I.plus}</button></div>
    ${src('lib', 'all', 'All Songs', plural(DOC.songs.length, 'song'))}
    ${DOC.libraries.map(l => src('lib', l.id, l.name, plural(l.songs.length, 'song'), dots('lib', l.id)))}
    ${catOn() ? html`<div class="sect-h sh-second"><span class="cap">Song catalog</span><span class="sh-what">free songs to find</span></div>${src('cat', 'wla', CATALOG.name, `${fmtN(catCount())} songs`)}` : ''}</div>`;
  let rows = '', add = '';
  if (state.src.type === 'pl') {
    let n = 0; const ic = { passage: I.book, media: I.film, mediaPlaylist: I.loop, placeholder: I.slot };
    rows = decks().map(d => {
      const pk = itemSel(), multi = pk.length > 1, rowCls = multi ? (pk.includes(d.key) ? (state.openItem === d.key ? 'on' : 'pickd') : state.openItem === d.key ? 'opn' : '') : state.openItem === d.key ? 'on' : '';
      if (d.kind === 'header') return hit(d.title) ? html`<button class="sh-hrow cap ${multi && pk.includes(d.key) ? 'pickd' : ''}" data-a="openItem:${d.key}" data-m="item:${d.key}" draggable="true" data-drag="item:${d.key}"><span class="hn">${d.title}</span><span class="sh-rowmore" data-a="menu:item,${d.key}" title="More">${I.more}</span></button>` : '';
      n++; if (!hit(d.title)) return '';
      const live = state.live && state.live.deck === d.key && !!state.on, ver = d.item && d.item.version && byId(DOC.versions, d.item.version);
      return html`<div class="item ${live ? 'live' : ''} ${d.kind === 'missing' || (d.kind === 'media' && !d.media) ? 'gone' : ''}"><button class="rowbtn ${rowCls}" data-a="openItem:${d.key}" data-m="item:${d.key}" draggable="true" data-drag="item:${d.key}">
        <span class="num mono">${two(n)}${ver ? html`<i class="sh-vbadge ${verLive(d) ? 'on' : ''}" title="Shows with the ${ver.name} version">${I.theme}</i>` : ''}</span><span class="t"><span class="tt">${d.title}</span>${d.slot ? html`<i class="sh-slot">${d.slot}</i>` : ''}${d.kind === 'missing' ? html` <i class="sh-slot">missing</i>` : ''}</span>
        ${ic[d.kind] ? html`<span class="ico">${ic[d.kind]}</span>` : ''}<span class="sh-rowmore" data-a="menu:item,${d.key}" title="More">${I.more}</span></button></div>`;
    });
    add = html`<button class="btn sh-add" data-a="menu:addItem">${I.plus} Add to playlist</button>`;
  } else if (state.src.type === 'lib') {
    const order = state.songOrder || 'listed', listed = librarySongs().filter(s => hit(s.title) || s.slides.some(sl => hit(allWords(sl))));
    if (order === 'az') listed.sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' })); if (order === 'slides') listed.sort((a, b) => b.slides.length - a.slides.length);
    const inLib = id => (DOC.libraries.find(l => l.songs.includes(id)) || {}).name;
    rows = listed.length ? listed.map(s =>
      html`<div class="item ${state.live && state.live.deck === 'lib-' + s.id && state.on ? 'live' : ''}"><button class="rowbtn ${state.openSong === s.id ? 'on' : ''} ${state.songPick.includes(s.id) ? 'pickd' : ''}" data-a="openSong:${s.id}" data-m="song:${s.id}" draggable="true" data-drag="song:${s.id}"><span class="ico">${I.song}</span><span class="t"><span class="tt">${s.title}</span>${state.changedOutside === s.id ? html`<i class="sh-slot sh-chg">changed</i>` : ''}${state.src.id === 'all' && inLib(s.id) ? html`<i class="sh-slot">${inLib(s.id)}</i>` : ''}</span><span class="meta mono">${s.slides.length || ''}</span><span class="sh-rowmore" data-a="menu:song,${s.id}" title="More">${I.more}</span></button></div>`)
      : html`<div class="sh-none">${f ? 'No songs match' : 'No songs here yet. Make one, import some, or drag songs onto this library’s name.'}</div>`;
    add = html`<div class="sh-order"><span class="cap">Order</span>${seg([['listed', 'As listed'], ['az', 'A–Z'], ['slides', 'Longest']], order, 'songOrder')}</div><button class="btn sh-add" data-a="newSong">${I.plus} New song</button>`;
  } else if (state.src.type === 'cat') ({ rows, add } = catLeft(hit));
  else rows = html`<div class="sh-none">The church’s VoxLive screens, one slide each. Click one to put its live captions on the screens.</div>`;
  put('left', html`<div class="sect items"><div class="sect-h"><span class="cap">${state.src.type === 'pl' ? 'Playlist' : state.src.type === 'vox' ? 'Integration' : state.src.type === 'cat' ? 'Song catalog' : 'Library'}</span><button class="iconbtn" data-a="menu:importMenu" title="Import songs or slides">${svg('M6.500 2v6.500M4 6l2.500 2.500L9 6M2.500 10.500h8')}</button></div>
    <button class="plbtn ${state.srcMenu ? 'open' : ''}" data-a="srcMenu" title="Choose a library or playlist (${keyText('⌘P')})"><i class="sh-kind ${state.src.type === 'pl' ? 'pl' : state.src.type === 'cat' ? 'cat' : 'lib'}">${state.src.type === 'pl' ? I.playlist : state.src.type === 'cat' ? I.catalog : I.library}</i><span>${srcName()}</span><span class="chev">${I.chev}</span></button>${panel}${rows}</div>
    <div class="sh-foot">${add}<input class="search" placeholder="Filter" data-i="filter" value="${state.filter}"></div>`);
  fitTitles(document.getElementById('left'));
}
// A list row's title takes up to two lines, broken where the chosen row's heavier weight (600) breaks them, so a row keeps its
// height whether it is chosen or not. A title still too long ends in … on its second line and shows whole in the row's tooltip.
// A tag after the title (missing, changed, a library's name) never shortens it: it follows the last line where there is room,
// takes a line of its own under a one-line title, and otherwise is left to the tooltip. The app lays its rows out the same way.
const titleRuler = document.createElement('canvas').getContext('2d');
function twoLines(text, room) {
  const fits = s => titleRuler.measureText(s).width <= room;
  if (fits(text)) return { lines: [text], cut: false };
  let first = '';
  for (const w of text.split(' ')) { const more = first ? `${first} ${w}` : w; if (fits(more)) first = more; else break; }
  if (!first) { let n = 1; while (n < text.length && fits(text.slice(0, n + 1))) n++; first = text.slice(0, n); }      // one word longer than the line: cut by letters
  const rest = text.slice(first.length).trimStart();
  if (fits(rest)) return { lines: [first, rest], cut: false };
  let n = rest.length; while (n > 0 && !fits(`${rest.slice(0, n).trimEnd()}…`)) n--;
  return { lines: [first, `${rest.slice(0, n).trimEnd()}…`], cut: true };
}
const titleTags = new WeakMap();      // a title's tags, kept so that it can be laid out again when its room changes
function fitTitles(root) {
  for (const tt of root.querySelectorAll('.rowbtn .tt')) {
    const t = tt.parentElement, row = t.closest('.rowbtn');
    if (!tt.dataset.full) { tt.dataset.full = tt.textContent.replace(/\s+/g, ' ').trim(); titleTags.set(tt, [...t.querySelectorAll(':scope > .sh-slot')]); }
    const full = tt.dataset.full, tags = titleTags.get(tt);
    tt.textContent = full; t.append(...tags); t.classList.remove('two'); row.removeAttribute('title');      // as drawn, before it is fitted
    const room = t.clientWidth;
    if (room <= 0 || !full) continue;
    const cs = getComputedStyle(tt);
    titleRuler.font = `600 ${cs.fontSize} ${cs.fontFamily}`;
    const tail = tags.reduce((w, g) => w + g.getBoundingClientRect().width + parseFloat(getComputedStyle(g).marginLeft || 0), 0);
    const { lines, cut } = twoLines(full, room), lastFits = titleRuler.measureText(lines[lines.length - 1]).width + tail <= room;
    const line = text => { const ln = document.createElement('span'); ln.className = 'ln'; ln.textContent = text; return ln; };
    tt.replaceChildren(...lines.map(line));
    let left = [];
    if (tags.length && lastFits) tt.lastChild.append(...tags);
    else if (tags.length && lines.length === 1) { const own = line(''); own.classList.add('tags'); own.append(...tags); tt.append(own); }
    else if (tags.length) { left = tags.map(g => g.textContent); tags.forEach(g => g.remove()); }
    t.classList.toggle('two', tt.children.length > 1);
    if (cut || left.length) row.title = [full, ...left].join(' · ');
  }
}
// The room changes with the window (a tablet turned round, a window made narrower): the titles are fitted again, at most once a
// frame. Only their lines are redrawn, so a field being typed in keeps its place.
let refitting = 0;
addEventListener('resize', () => { if (!refitting) refitting = requestAnimationFrame(() => { refitting = 0; if (state.mode === 'show') fitTitles(document.getElementById('left')); }); });
A.srcMenu = () => { state.srcMenu = !state.srcMenu; renderShowLeft(); };
A.songOrder = ([o]) => { state.songOrder = o; renderShowLeft(); };
A.emptyAdd = ([what]) => { const it = M.addItem().find(x => x.t.startsWith({ song: 'A song', passage: 'A Bible', header: 'A header' }[what])); if (it) it.f(); };
A.src = ([type, id]) => {
  state.src = { type, id }; state.srcMenu = false; state.filter = ''; state.picked = { deck: null, set: [] };
  deckCache = null;
  if (type === 'pl') state.openItem = (decks().find(d => d.slides && d.slides.length) || {}).key || null;
  if (type === 'lib' && !librarySongs().some(s => s.id === state.openSong)) state.openSong = (librarySongs()[0] || {}).id || null;
  if (type === 'cat') { deckCache = null; state.openCat = (catSongs().find(s => s.id === state.openCat) || catSongs()[0] || {}).id || null; }
  render(); 
};
// Picking items the Finder way (as the app has it, 21 November): a click picks one and is the anchor; Shift-click takes the
// range from the anchor, in place of what was picked; ⌘-click adds or takes away one and moves the anchor. Shift-↑/↓ and ⌘A too.
const itemIds = () => playlist().items.map(i => i.id);
const itemSel = () => { if (state.src.type !== 'pl') return []; const ids = itemIds(), p = (state.itemPick || []).filter(id => ids.includes(id)); return p.length ? p : state.openItem && ids.includes(state.openItem) ? [state.openItem] : []; };
const itemRange = (a, b) => { const ids = itemIds(), i = ids.indexOf(a), j = ids.indexOf(b); return i < 0 || j < 0 ? [b] : ids.slice(Math.min(i, j), Math.max(i, j) + 1); };
A.openItem = ([id, how], _el, ev) => {      // opening an item only brings it into view; nothing changes on the screens
  if (ev && (ev.metaKey || ev.ctrlKey)) { const p = itemSel(); state.itemPick = p.includes(id) ? p.filter(x => x !== id) : [...p, id]; state.itemAnchor = id; return renderShowLeft(); }
  if (ev && ev.shiftKey) { state.itemPick = itemRange(state.itemAnchor || state.openItem || id, id); state.itemCursor = id; return renderShowLeft(); }
  state.itemPick = [id]; state.itemAnchor = state.itemCursor = id; state.mediaPick = [];
  state.openItem = id; if (how === 'key') cueFirst(id); document.body.classList.remove('nav-open'); render();
  scrollToDeck(id); const el = $('deck-' + id); if (el) { el.classList.add('opened'); setTimeout(() => el.classList.remove('opened'), 1400); }
};
A.openSong = ([id], el, ev) => {
  if (ev && (ev.metaKey || ev.ctrlKey)) { state.songPick.includes(id) ? state.songPick.splice(state.songPick.indexOf(id), 1) : state.songPick.push(id); return renderShowLeft(); }
  if (ev && ev.shiftKey) { const list = librarySongs().map(s => s.id), a = list.indexOf(state.openSong), b = list.indexOf(id); state.songPick = list.slice(Math.min(a, b), Math.max(a, b) + 1); return renderShowLeft(); }
  state.songPick = []; state.openSong = id; deckCache = null; if (el === 'key') cueFirst('lib-' + id); render();
};
A.newLibrary = () => ask('New library', 'Name', '', v => mutate(() => { const l = { id: newId('lib'), name: v, songs: [] }; DOC.libraries.push(l); state.src = { type: 'lib', id: l.id }; state.srcMenu = false; }), { placeholder: 'Christmas' });
A.newPlaylist = () => ask('New playlist', 'Name', '', v => mutate(() => { const p = { id: newId('pl'), name: v, items: [] }; DOC.playlists.push(p); state.src = { type: 'pl', id: p.id }; state.srcMenu = false; state.openItem = null; }), { placeholder: 'Sunday Morning' });
M.lib = ([id]) => { const l = byId(DOC.libraries, id); return [
  { t: 'Rename', f: () => renameAt(state.src.type === 'lib' && state.src.id === l.id ? '.plbtn > span' : `[data-a="src:lib,${l.id}"] .t`, l.name, v => mutate(() => { l.name = v; }), 'Rename library') },
  { sep: true }, { t: 'Delete Library', f: () => undoable(`Delete “${l.name}”?`, 'The songs stay in All Songs.', 'Delete', () => mutate(() => { DOC.libraries.splice(DOC.libraries.indexOf(l), 1); if (state.src.id === id) state.src = { type: 'lib', id: 'all' }; })) }]; };
M.pl = ([id]) => { const p = byId(DOC.playlists, id); return [
  { t: 'Rename', f: () => renameAt(state.src.type === 'pl' && state.src.id === p.id ? '.plbtn > span' : `[data-a="src:pl,${p.id}"] .t`, p.name, v => mutate(() => { p.name = v; }), 'Rename playlist') },
  { t: 'Duplicate', f: () => mutate(() => { const c = clone(p); c.id = newId('pl'); c.name += ' copy'; c.items.forEach(i => { i.id = newId('it'); }); DOC.playlists.push(c); }) },
  { t: 'Export…', f: () => exportSheet(p.name, p.items.filter(i => i.song).map(i => i.song)) },
  { sep: true }, { t: 'Delete Playlist', dim: DOC.playlists.length === 1, f: () => undoable(`Delete “${p.name}”?`, 'Its songs stay in the library.', 'Delete', () => mutate(() => { const k = DOC.playlists.indexOf(p); DOC.playlists.splice(k, 1); if (state.src.type === 'pl' && state.src.id === id) state.src = { type: 'pl', id: (DOC.playlists[k - 1] || DOC.playlists[k] || DOC.playlists[0]).id }; })) }]; };      // the one in front: the one before it opens, or the next if it was first

/* ───────── playlist items ───────── */
const itemById = id => byId(playlist().items, id);
// An item set to show with a version: a pill with the version's name on its row, lit (on screen) while it is the one up
const verLive = d => !!(d.item && d.item.version && state.version && state.version.id === d.item.version && state.version.from === d.key && state.on);
const verPill = d => d.item && d.item.version && byId(DOC.versions, d.item.version) ? html`<i class="sh-ver ${verLive(d) ? 'on' : ''}" title="Shows with the ${versionName(d.item.version)} version">${versionName(d.item.version)}</i>` : '';
// The easy way to set it: a version button among the song's (or passage's) title-bar tools, lit when one is set; one click
// opens the choice. Shown only when a theme has a version.
const verTool = d => !DOC.versions.length || !d.item ? '' : html`<button class="iconbtn ${d.item.version ? 'vset' : ''} ${verLive(d) ? 'vlive' : ''}" data-a="menu:itemVersion,${d.key}" title="${d.item.version ? `Shows with the ${versionName(d.item.version)} version${d.item.versionKeep ? ', kept after it' : ''}. Click to change` : 'Show this with a theme version, such as ' + DOC.versions[0].name}" aria-label="Theme version">${I.theme}</button>`;
function pickSong(title, done) {
  openSheet({ title, body: html`<div class="ui-list">${DOC.songs.map(s => html`<button class="rowbtn" data-a="pickSongRow:${s.id}"><span class="ico">${I.song}</span><span class="t">${s.title}</span><span class="meta mono">${state.src.type === 'pl' && playlist().items.some(x => x.song === s.id) ? 'In this playlist · ' : ''}${s.slides.length} slides</span></button>`)}</div>`, buttons: [{ t: 'Cancel' }] });
  state.sheet.done = done;
}
A.pickSongRow = ([id]) => { const done = state.sheet.done; closeSheet(); done(id); };
const addItem = it => mutate(() => { const items = playlist().items, at = items.findIndex(i => i.id === state.openItem); items.splice(at < 0 ? items.length : at + 1, 0, { id: newId('it'), ...it }); });
M.addItem = () => [
  { t: 'A song…', f: () => pickSong('Add a song', id => addItem({ kind: 'song', song: id })) },
  { t: 'A Bible passage…', f: () => { state.mode = 'bible'; render(); refuse('Look a passage up, then Add to playlist'); } },
  { t: 'A header…', f: () => ask('New header', 'Name', '', v => addItem({ kind: 'header', name: v }), { placeholder: 'Worship' }) },
  { t: 'A placeholder…', f: () => ask('New placeholder', 'What it keeps a place for', '', v => addItem({ kind: 'placeholder', name: v, song: null }), { placeholder: 'Sermon slides' }) },
  { t: 'A picture or video', sub: DOC.media.map(m => ({ t: m.name + (playlist().items.some(x => x.media === m.id) ? ' · in this playlist' : ''), f: () => addItem({ kind: 'media', media: m.id }) })) },
  { t: 'A media playlist', sub: DOC.mediaPlaylists.map(m => ({ t: m.name + (playlist().items.some(x => x.mp === m.id) ? ' · in this playlist' : ''), f: () => addItem({ kind: 'mediaPlaylist', mp: m.id }) })) }];
M.item = ([id]) => {
  const items = playlist().items, it = itemById(id), n = items.indexOf(it), s = it.song && songById(it.song);
  const pk = itemSel().includes(id) ? itemSel() : [id], sorted = items.filter(x => pk.includes(x.id));      // with several picked, the menu acts on all of them
  const move = by => mutate(() => { const L = playlist().items, order = by < 0 ? sorted : [...sorted].reverse();
    for (const x of order) { const i = L.indexOf(x), j = i + by; if (j < 0 || j >= L.length || pk.includes(L[j].id)) continue; L.splice(i, 1); L.splice(j, 0, x); } });
  const edgeUp = items.indexOf(sorted[0]) === 0, edgeDown = items.indexOf(sorted[sorted.length - 1]) === items.length - 1;
  return [
    s && { t: 'Add Slide', f: () => mutate(() => { const last = s.slides[s.slides.length - 1]; s.slides.push(slide(last ? last.group : 'Verse 1', 'New slide', '')); }) },
    s && { t: 'Song', sub: M.song([s.id, playlist().id, id]) },
    (it.kind === 'header' || it.kind === 'placeholder') && { t: 'Rename', f: () => renameAt(it.kind === 'header' ? `#left .sh-hrow[data-drag="item:${id}"] .hn` : `#left [data-drag="item:${id}"] .t`, it.name, v => mutate(() => { it.name = v; }), 'Rename') },
    it.kind === 'placeholder' && { t: 'Fill with Song…', f: () => A.fillSlot([id]) },
    it.kind === 'passage' && { t: 'Open on the Bible Page', f: () => { lookUp({ ...it.passage }); state.mode = 'bible'; render(); } },
    it.kind === 'placeholder' && it.song && { t: 'Empty It', f: () => mutate(() => { it.song = null; }) },
    // the owner: "clicking the three buttons and have a 'version' section in the dropdown": the versions right in the menu, one click
    ...(DOC.versions.length > 0 && (it.kind === 'song' || it.kind === 'passage' || (it.kind === 'placeholder' && it.song)) ? [{ head: 'Version' }, ...M.itemVersion([id]).slice(1, -1).filter(i => !i.sep), { sep: true }] : []),
    { t: pk.length > 1 ? `Move ${pk.length} Up` : 'Move Up', dim: edgeUp, f: () => !edgeUp && move(-1) }, { t: pk.length > 1 ? `Move ${pk.length} Down` : 'Move Down', dim: edgeDown, f: () => !edgeDown && move(1) },
    { sep: true }, (() => { const all = itemSel().includes(id) ? itemSel() : [id]; return { t: all.length > 1 ? `Remove ${all.length} Items from Playlist` : 'Remove from Playlist', f: () => removeItems(all) }; })(),
    s && { t: 'Delete Song from Library…', f: () => { const live = state.live && state.on && decks().some(d => d.key === state.live.deck && d.song === s);
      confirmDo(`Delete “${s.title}”?`, `${live ? 'It’s on the screens now; it comes off. ' : ''}It leaves every library; playlists keep its place and show it as missing. This can’t be undone.`, 'Delete', () => mutate(() => { DOC.songs.splice(DOC.songs.indexOf(s), 1); DOC.libraries.forEach(l => { l.songs = l.songs.filter(x => x !== s.id); }); if (live) clear('slide'); })); } }];
};
// Delete in a playlist (and the menu's Remove): the picked items go at once, never a song itself; what is on screen stays on
// screen; ⌘Z puts them back where they were, still picked. In a library Delete still asks.
function removeItems(ids) { const p = playlist(), gone = p.items.filter(i => ids.includes(i.id)); if (!gone.length) return;
  if (state.live && ids.includes(state.live.deck)) state.heldDeck = deckByKey(state.live.deck);      // it stays on screen, and Space and the arrows go on through it
  mutate(() => { p.items = p.items.filter(i => !ids.includes(i.id)); if (ids.includes(state.openItem)) state.openItem = null; }); state.itemPick = gone.map(i => i.id);
   }
A.fillSlot = ([id]) => pickSong('Choose a song', sid => { if (playlist().items.some(x => x.song === sid && x.id !== id)) return refuse(`${songById(sid).title} is already in ${playlist().name}.`); mutate(() => { itemById(id).song = sid; }); });

/* ───────── songs ───────── */
function parseSongText(text) {
  const drafts = []; let group = 'Verse 1', cur = null, inTr = false;
  for (const raw of text.replace(/\r/g, '').split('\n')) {
    const line = raw.trimEnd(), head = /^\[(.+)\]$/.exec(line.trim());
    if (head) { group = head[1].trim(); cur = null; continue; }
    if (!line.trim()) { cur = null; continue; }
    if (!cur) { cur = { group, pri: [], tr: [] }; drafts.push(cur); inTr = false; }
    if (line.trim() === '--') { inTr = true; continue; }
    (inTr ? cur.tr : cur.pri).push(line);
  }
  return drafts;
}
function slidesFromDrafts(drafts, old) {   // rebuilt slides keep their looks, labels and notes
  return drafts.map((d, n) => {
    const s = old && old[n] ? clone(old[n]) : old && old.length ? { ...clone(old[old.length - 1]), label: '', notes: '', hotkey: '', actions: [] } : slide(d.group, '', '');
    s.id = newId('s'); s.objects.forEach(o => { o.id = newId('o'); }); s.group = d.group;
    let main = s.objects.find(o => o.role === 'main'); if (!main) { main = words('main', ''); s.objects.unshift(main); }
    setWords(main, d.pri.join('\n'));
    const tr = s.objects.find(o => o.role === 'translation');
    if (d.tr.length) { if (tr) setWords(tr, d.tr.join('\n')); else s.objects.push(words('translation', d.tr.join('\n'))); } else if (tr) s.objects.splice(s.objects.indexOf(tr), 1);
    return s;
  });
}
function songText(s) {
  let out = '', last = null;
  s.slides.forEach((sl, k) => { if (k > 0) out += '\n'; if (sl.group !== last) out += `[${sl.group}]\n`; last = sl.group; const tr = slideWords(sl, 'translation'); out += slideWords(sl) + '\n' + (sl.objects.some(o => o.role === 'translation') && tr ? `--\n${tr}\n` : ''); });
  return out;
}
A.newSong = () => openSheet({ title: 'New song', wide: true,
  body: html`${field('Title', html`<input class="field" id="nsTitle" placeholder="Song title">`)}${field('Theme', html`<select class="sel" id="nsTheme">${options([['', 'The songs theme'], ...DOC.themes.map(t => [t.id, t.name])], '')}</select>`)}${field('Words', html`<textarea class="ui-ta" id="nsWords" placeholder="[Verse 1]&#10;First line&#10;Second line&#10;&#10;[Chorus]&#10;…"></textarea>`)}`,
  note: '[Group] starts a group · a blank line starts a slide · -- starts the translation',
  buttons: [{ t: 'Cancel' }, { t: 'Make the song', primary: true, f: () => {
    const title = $('nsTitle').value.trim(); if (!title) return false;
    const drafts = parseSongText($('nsWords').value), theme = $('nsTheme').value || null;
    mutate(() => { const s = song(title, []); s.theme = theme; s.slides = drafts.length ? slidesFromDrafts(drafts) : [slide('Verse 1', '', '')]; DOC.songs.push(s);
      if (state.src.type === 'lib' && state.src.id !== 'all') byId(DOC.libraries, state.src.id).songs.push(s.id);
      if (state.src.type === 'pl') { playlist().items.push({ id: newId('it'), kind: 'song', song: s.id }); state.openItem = playlist().items[playlist().items.length - 1].id; } else state.openSong = s.id; });
    
  } }] });
A.songInfo = ([id]) => { const s = songById(id), i = s.info; openSheet({ title: `${s.title} · details`,
  body: html`${catInfo(s)}${[['artist', 'Artist'], ['author', 'Author'], ['publisher', 'Publisher'], ['year', 'Year'], ['ccli', 'CCLI Song #']].map(([k, l]) => field(l, html`<input class="field" id="si-${k}" value="${i[k] == null ? '' : i[k]}">`))}<p class="ui-p sub" style="margin-top:10px">${copyrightLine(s) || 'The copyright line is built from these'}</p>`,
  buttons: [{ t: 'Cancel' }, { t: 'Save', primary: true, f: () => mutate(() => { for (const k of Object.keys(i)) i[k] = $('si-' + k).value.trim(); }) }] }); };
A.songNotes = ([id]) => { const s = songById(id); openSheet({ title: `${s.title} · operator notes`, body: html`<textarea class="ui-ta" id="snText" placeholder="Notes for whoever runs the slides">${s.notes || ''}</textarea>`,
  buttons: [{ t: 'Cancel' }, { t: 'Save', primary: true, f: () => mutate(() => { s.notes = $('snText').value; }) }] }); };
A.songWords = ([id]) => { const d = decks().find(x => x.song && x.song.id === id); enterEdit(d ? d.key : null, 0, id); state.reflow = true; render(); };
A.songArr = ([id]) => { state.arrOpen[id] = !state.arrOpen[id]; render(); };
const usualTrans = () => { const t = DOC.settings.transition; return t.kind === 'Cut' ? 'Usual (Cut)' : `Usual (${t.kind}, ${t.time} s)`; };
const transitionSub = (now, set) => [{ t: usualTrans(), on: !now, f: () => set(null) }, ...TRANSITIONS.map(k => ({ t: k, on: now && now.kind === k, f: () => set({ kind: k, time: k === 'Cut' ? 0 : .7, dir: 'Left' }) }))];
// From a playlist's item, the song's menu leaves that playlist out of Add to Playlist (the owner: "there shouldnt be an option
// to add it to the playlist its already in"); from a library, every playlist is offered.
M.song = ([id, inPl, itemId]) => { const s = songById(id), pk = itemId && itemSel().includes(itemId) ? itemSel().map(x => (itemById(x) || {}).song).filter(Boolean) : [id]; return [
  DOC.playlists.every(p => p.id === inPl) ? { t: 'Add to New Playlist…', f: () => ask('New playlist', 'Name', '', v => { mutate(() => { DOC.playlists.push({ id: newId('pl'), name: v, items: pk.map(sid => ({ id: newId('it'), kind: 'song', song: sid })) }); }); toast(`Added ${pk.length > 1 ? pk.length + ' songs' : s.title} to ${v}.`); }, { placeholder: 'Sunday Evening' }) } :
  { t: 'Add to Playlist', sub: [...DOC.playlists.filter(p => p.id !== inPl).map(p => ({ t: p.name + (p.items.some(x => x.song === id) ? ' · in it already' : ''), f: () => { mutate(() => { pk.forEach(sid => p.items.push({ id: newId('it'), kind: 'song', song: sid })); }); toast(pk.length > 1 ? `Added ${pk.length} songs to ${p.name}.` : `Added ${s.title} to ${p.name}.`); } })), { sep: true },
    { t: 'New Playlist…', f: () => ask('New playlist', 'Name', '', v => mutate(() => { DOC.playlists.push({ id: newId('pl'), name: v, items: [{ id: newId('it'), kind: 'song', song: id }] }); toast(`Made ${v} with ${s.title} in it`); }), { placeholder: 'Sunday Evening' }) }] },
  { t: 'Libraries', sub: DOC.libraries.map(l => ({ t: l.name, on: l.songs.includes(id), f: () => mutate(() => { l.songs.includes(id) ? l.songs.splice(l.songs.indexOf(id), 1) : l.songs.push(id); }) })) },
  { sep: true },
  { t: 'Rename', f: () => { const d = decks().find(x => x.song && x.song.id === s.id && x.key === state.openItem) || decks().find(x => x.song && x.song.id === s.id);      // on its title bar, else its row in the list
    renameAt((d && document.querySelector(`[id="deck-${d.key}"] > b`)) || document.querySelector(`[data-a="openSong:${s.id}"] .t`), s.title, v => mutate(() => { s.title = v; }), 'Rename song', 'Title'); } },
  { t: 'Duplicate', f: () => mutate(() => { const c = clone(s); c.id = newId('song'); c.title += ' copy'; c.slides.forEach(sl => { sl.id = newId('s'); sl.objects.forEach(o => { o.id = newId('o'); }); });      // beside the original: in the song list, its libraries and this playlist
    DOC.songs.splice(DOC.songs.indexOf(s) + 1, 0, c); DOC.libraries.forEach(l => { const k = l.songs.indexOf(s.id); if (k >= 0) l.songs.splice(k + 1, 0, c.id); });
    if (itemId) { const L = playlist().items, k = L.findIndex(x => x.id === itemId); if (k >= 0) L.splice(k + 1, 0, { id: newId('it'), kind: 'song', song: c.id }); } }) },
  { t: 'Details…', f: () => A.songInfo([id]) }, { t: 'Words…', f: () => A.songWords([id]) },
  { t: 'Theme', sub: [{ t: 'The songs theme', on: !s.theme, f: () => mutate(() => { s.theme = null; }) }, ...DOC.themes.map(t => ({ t: t.name, on: s.theme === t.id, f: () => mutate(() => { s.theme = t.id; }) }))] },
  { t: 'Transition', sub: transitionSub(s.transition, v => mutate(() => { s.transition = v; })) },
  { t: 'Loop to First Slide', on: s.loop, f: () => mutate(() => { s.loop = !s.loop; }) },
  { t: 'Play on Announcements Layer', f: () => { state.ann = { song: id, q: 0, left: s.slides[0].advance || 8 }; state.annOffSlides = false; render();  } },
  { sep: true }, { t: 'Export…', f: () => exportSheet(s.title, [id]) },
  !inPl && { t: 'Delete…', f: () => confirmDo(`Delete “${s.title}”?`, 'It leaves every library. Playlists keep its place and show it as missing. This can’t be undone.', 'Delete', () => mutate(() => { DOC.songs.splice(DOC.songs.indexOf(s), 1); DOC.libraries.forEach(l => { l.songs = l.songs.filter(x => x !== id); }); if (state.openSong === id) state.openSong = (DOC.songs[0] || {}).id; })) }]; };

/* arrangements: the groups in the order they are sung */
const arrOf = s => s.arrangement && byId(s.arrangements, s.arrangement);
M.arrPick = ([id]) => { const s = songById(id), a = arrOf(s); return [
  { t: 'As written', on: !a, f: () => mutate(() => { s.arrangement = null; }) },
  ...s.arrangements.map(x => ({ t: x.name, on: a === x, f: () => mutate(() => { s.arrangement = x.id; }) })), { sep: true },
  { t: 'New Arrangement…', f: () => ask('New arrangement', 'Name', '', v => mutate(() => { const x = { id: newId('arr'), name: v, groups: [...new Set(s.slides.map(sl => sl.group))] }; s.arrangements.push(x); s.arrangement = x.id; }), { placeholder: 'As sung' }) },
  a && { t: 'Rename', f: () => renameAt(`.sh-arr[data-arr="${s.id}"] .sh-arrname`, a.name, v => mutate(() => { a.name = v; }), 'Rename arrangement') },
  a && { t: 'Delete', f: () => mutate(() => { s.arrangements.splice(s.arrangements.indexOf(a), 1); s.arrangement = null; }) }]; };
M.arrAdd = ([id]) => { const s = songById(id); return [...new Set(s.slides.map(sl => sl.group))].map(g => ({ t: g, f: () => mutate(() => { arrOf(s).groups.push(g); }) })); };
// A group sung several times running is one piece marked ×2, ×3…; underneath it is simply the group listed that many times.
const arrRuns = groups => groups.reduce((r, g, i) => { const last = r[r.length - 1]; if (last && last.g === g) last.count++; else r.push({ g, start: i, count: 1 }); return r; }, []);
function arrMove(groups, from, to, after) {       // the piece that starts at `from` goes just before, or just after, the piece that starts at `to`
  const runs = arrRuns(groups), a = runs.findIndex(r => r.start === from), target = runs.find(r => r.start === to); if (a < 0 || !target || runs[a] === target) return;
  const piece = runs.splice(a, 1)[0]; runs.splice(runs.indexOf(target) + (after ? 1 : 0), 0, piece); groups.splice(0, groups.length, ...runs.flatMap(r => Array(r.count).fill(r.g)));
}
M.arrChip = ([id, n]) => { const s = songById(id), g = arrOf(s).groups, runs = arrRuns(g), at = runs.findIndex(r => r.start === +n), run = runs[at]; if (!run) return [];
  const times = c => mutate(() => { g.splice(run.start, run.count, ...Array(c).fill(run.g)); });
  return [{ head: 'How many times' }, { t: 'Once', on: run.count === 1, f: () => times(1) }, ...[2, 3, 4].map(c => ({ t: '×' + c, on: run.count === c, f: () => times(c) })),
    { t: run.count > 4 ? `×${run.count} · change…` : 'More…', on: run.count > 4, f: () => ask('How many times', 'Times', String(Math.max(5, run.count)), v => { const c = Math.round(+v); if (c >= 1 && c <= 20) times(c); else refuse('A number from 1 to 20'); }) }, { sep: true },
    { t: 'Move Left', dim: at === 0, f: () => mutate(() => arrMove(g, run.start, runs[at - 1].start, false)) }, { t: 'Move Right', dim: at === runs.length - 1, f: () => mutate(() => arrMove(g, run.start, runs[at + 1].start, true)) },
    { t: 'Remove', dim: runs.length === 1, f: () => mutate(() => { g.splice(run.start, run.count); }) }]; };
A.arrGo = ([key, n]) => { const d = deckByKey(key), a = arrOf(d.song); let q = 0; for (let i = 0; i < +n; i++) q += d.song.slides.filter(sl => sl.enabled && sl.group === a.groups[i]).length; if (q < d.seq.length) go(key, q); };

/* ───────── slides: picking, the menu, and what it does ───────── */
function pickSlide(key, s, run) {
  const p = state.picked.deck === key ? state.picked : { deck: key, set: [], anchor: s };
  if (run && p.anchor != null) { const [a, b] = [Math.min(p.anchor, s), Math.max(p.anchor, s)]; p.set = []; for (let i = a; i <= b; i++) p.set.push(i); }
  else { p.set.includes(s) ? p.set.splice(p.set.indexOf(s), 1) : p.set.push(s); p.anchor = s; }
  state.picked = p; render();
}
const targets = (key, s) => state.picked.deck === key && state.picked.set.includes(s) ? [...state.picked.set].sort((a, b) => a - b) : [s];
A.goAt = ([key, q]) => { state.picked = { deck: null, set: [] }; state.cue = null; go(key, +q); };
A.go = ([key, s], el, ev) => { if (ev && (ev.metaKey || ev.ctrlKey || ev.shiftKey)) return pickSlide(key, +s, ev.shiftKey); state.picked = { deck: null, set: [] }; state.cue = null; go(key, null, +s); };      // a click puts that slide on and drops the cue
A.editSlide = ([key, s]) => { if (!key.startsWith('cat-')) return enterEdit(key, +s);      // a catalog song edited joins the library first
  let j; mutate(() => { j = catJoin(key.slice(8)); }); toast(`“${j.title}” is in your library now, to change as you like`); enterEdit(null, +s, j.id); };
function slideOp(key, s, what) {
  const d = deckByKey(key), song = d.song, idx = targets(key, s), sl = song.slides;
  if (what === 'copy' || what === 'cut') { state.clip = { type: 'slides', slides: idx.map(i => clone(sl[i])) }; if (what === 'copy') return toast(`${idx.length} slide${idx.length > 1 ? 's' : ''} copied`); }
  if (what === 'style') { const m = sl[s].objects.find(o => o.role === 'main'); state.clip = { type: 'style', style: clone((m && m.style) || {}), frame: m && m.frame ? clone(m.frame) : null }; return toast('Text style copied'); }
  mutate(() => {
    const fresh = x => { const c = clone(x); c.id = newId('s'); c.objects.forEach(o => { o.id = newId('o'); }); return c; };
    if (what === 'cut' || what === 'delete') [...idx].reverse().forEach(i => sl.splice(i, 1));
    if (what === 'paste' && state.clip && state.clip.type === 'slides') sl.splice(idx[idx.length - 1] + 1, 0, ...state.clip.slides.map(fresh));
    if (what === 'pastestyle' && state.clip && state.clip.type === 'style') idx.forEach(i => { const m = sl[i].objects.find(o => o.role === 'main'); if (m) { m.style = clone(state.clip.style); if (state.clip.frame) m.frame = clone(state.clip.frame); } });
    if (what === 'duplicate') sl.splice(idx[idx.length - 1] + 1, 0, ...idx.map(i => fresh(sl[i])));
    if (what === 'up' && idx[0] > 0) { const x = sl.splice(idx[0] - 1, 1)[0]; sl.splice(idx[idx.length - 1], 0, x); }
    if (what === 'down' && idx[idx.length - 1] < sl.length - 1) { const x = sl.splice(idx[idx.length - 1] + 1, 1)[0]; sl.splice(idx[0], 0, x); }
    if (!sl.length) sl.push(slide('Verse 1', '', ''));
    state.picked = { deck: null, set: [] };
  });
}
const setSlides = (key, s, fn) => mutate(() => targets(key, s).forEach(i => fn(deckByKey(key).song.slides[i])));
M.slide = ([key, s]) => {
  s = +s; const d = deckByKey(key), sl = d.slides[s];
  if (!d.song) return [{ t: 'Show', f: () => go(key, null, s) }, d.kind === 'passage' && { t: 'Open on the Bible Page', f: () => { const it = d.item; if (it) state.bible.ref = { ...it.passage }; state.mode = 'bible'; render(); } }];
  const n = targets(key, s).length, th = themeFor(d.song);
  const stepChoices = [...['all', 'slide', 'media', 'props', 'messages', 'audio'].map(w => ({ do: 'clear', what: w })), ...DOC.messages.map(m => ({ do: 'message', what: 'show', name: m.name })), { do: 'message', what: 'hide' },
    ...DOC.props.flatMap(p => [{ do: 'prop', what: 'show', name: p.name }, { do: 'prop', what: 'hide', name: p.name }]), ...DOC.timers.flatMap(t => ['start', 'stop', 'reset'].map(w => ({ do: 'timer', what: w, name: t.name }))),
    { do: 'audience', on: true }, { do: 'audience', on: false }, ...DOC.media.map(m => ({ do: 'media', id: m.id }))];
  return [
    { t: 'Edit', f: () => enterEdit(key, s) }, { sep: true },
    { t: 'Cut', k: '⌘X', f: () => slideOp(key, s, 'cut') }, { t: 'Copy', k: '⌘C', f: () => slideOp(key, s, 'copy') },
    { t: 'Paste', k: '⌘V', dim: !state.clip || state.clip.type !== 'slides', f: () => slideOp(key, s, 'paste') },
    { t: 'Duplicate', f: () => slideOp(key, s, 'duplicate') }, { t: n > 1 ? `Delete ${n} Slides` : 'Delete', f: () => undoable(n > 1 ? `Delete ${n} slides?` : 'Delete this slide?', 'Undo brings them back.', 'Delete', () => slideOp(key, s, 'delete')) },
    { t: 'Move Up', dim: s === 0, f: () => slideOp(key, s, 'up') }, { t: 'Move Down', dim: s === d.slides.length - 1, f: () => slideOp(key, s, 'down') }, { sep: true },
    { t: 'Group', sub: DOC.groups.map(g => ({ t: g.name, on: sl.group === g.name, f: () => setSlides(key, s, x => { x.group = g.name; }) })) },
    { t: 'Label', sub: [{ t: 'None', on: !sl.label, f: () => setSlides(key, s, x => { x.label = ''; }) }, ...LABELS.map(l => ({ t: l, on: sl.label === l, f: () => setSlides(key, s, x => { x.label = l; }) })),
      { t: 'Another…', f: () => ask('Label', 'Label', sl.label, v => setSlides(key, s, x => { x.label = v; }), { allowEmpty: true }) }] },
    { t: 'Transition', sub: transitionSub(sl.transition, v => setSlides(key, s, x => { x.transition = v; })) },
    { t: sl.hotkey ? `Hot Key: ${sl.hotkey}` : 'Hot Key…', f: () => ask('Hot key', 'A letter or digit that puts this slide on screen', sl.hotkey, v => mutate(() => { sl.hotkey = v.slice(0, 1).toUpperCase(); }), { allowEmpty: true }) },
    { t: 'Go to Next', sub: [0, 3, 5, 8, 15, 30].map(t => ({ t: t ? `After ${t} seconds` : 'None', on: (sl.advance || 0) === t, f: () => setSlides(key, s, x => { x.advance = t; }) })) },
    { t: 'Template', sub: [{ t: 'Usual (the theme picks)', on: !sl.template, f: () => setSlides(key, s, x => { x.template = null; }) }, ...th.templates.filter(t => t.for !== 'bible').map(t => ({ t: t.name, on: sl.template === t.id, f: () => setSlides(key, s, x => { x.template = t.id; }) }))] },
    { t: 'Media', sub: [{ t: 'None', on: !sl.media, f: () => setSlides(key, s, x => { x.media = null; }) }, ...DOC.media.map(m => ({ t: m.name, on: sl.media === m.id, f: () => setSlides(key, s, x => { x.media = m.id; }) }))] },
    { t: `Actions${sl.actions.length ? ` (${sl.actions.length})` : ''}`, sub: [...sl.actions.map((a, i) => ({ t: `Remove: ${stepText(a)}`, f: () => mutate(() => { sl.actions.splice(i, 1); }) })), sl.actions.length && { sep: true },
      { t: 'Add an Action', sub: stepChoices.map(c => ({ t: stepText(c), f: () => mutate(() => { sl.actions.push(c); }) })) }] },
    { t: sl.enabled ? 'Disable' : 'Enable', f: () => setSlides(key, s, x => { x.enabled = !sl.enabled; }) }, sl.check && { t: 'Mark as Checked', f: () => setSlides(key, s, x => { delete x.check; }) }, { sep: true },
    { t: 'Copy Style', f: () => slideOp(key, s, 'style') }, { t: 'Paste Style', dim: !state.clip || state.clip.type !== 'style', f: () => state.clip && state.clip.type === 'style' && slideOp(key, s, 'pastestyle') }];
};

/* ───────── middle ───────── */
function slideTile(d, sl, s, at) {      // `at`, when the song has an arrangement: where this tile stands in the order sung
  const q = at ? at.q : -1, again = !!(at && at.again), num = two((q >= 0 ? q : s) + 1);
  const first = at ? at.prev !== sl.group : s === 0 || d.slides[s - 1].group !== sl.group, name = [sl.group, sl.label].filter(Boolean).join(' · ');
  const picked = state.picked.deck === d.key && state.picked.set.includes(s);
  const badges = [sl.check ? html`<i class="sh-b chk" title="${sl.check}. Its menu has Mark as Checked">Check</i>` : '', sl.hotkey ? html`<i class="sh-b key">${sl.hotkey}</i>` : '', sl.advance ? html`<i class="sh-b">${sl.advance}s</i>` : '', sl.actions && sl.actions.length ? html`<i class="sh-b">${sl.actions.length} act</i>` : '', sl.transition ? html`<i class="sh-b">${sl.transition.kind}</i>` : ''];
  const colour = sl.bible || sl.vox ? 'var(--c-bible)' : groupColour(sl.group), cued = isCued(d.key, q, s), cls = `${(q >= 0 ? !!state.live && !!state.on && state.live.deck === d.key && state.live.q === q : isLive(d.key, s)) ? 'live' : ''} ${cued ? 'cued' : ''} ${again ? 'again' : ''} ${at && at.out ? 'out' : ''} ${picked ? 'pickd' : ''} ${sl.enabled === false ? 'off' : ''} ${first || !d.song ? '' : 'cont'}`;
  const drag = d.song && !again && !d.key.startsWith('cat-'), a = { 'data-a': again ? `goAt:${d.key},${q}` : `go:${d.key},${s}`, 'data-m': `slide:${d.key},${s}`, draggable: drag ? 'true' : null, 'data-drag': drag ? `slide:${d.key},${s}` : null }, v = DOC.settings.view;
  if (v === 'easy') return html`<button class="sh-easy ${cls}" style="--g:${colour}" ${a}><span class="n mono">${num}</span><i class="dot"></i><span class="w">${slideWords(sl).replace(/\n/g, ' / ')}</span><span class="g">${name}</span><span class="edit" data-a="menu:slide,${d.key},${s}" title="More">${I.more}</span></button>`;
  if (v === 'table') return html`<button class="sh-trow ${cls}" style="--g:${colour}" ${a}><span class="n mono">${num}</span><span class="g"><i class="dot"></i>${name}</span><span class="w">${slideWords(sl).replace(/\n/g, ' / ')}</span><span class="nt">${sl.notes || ''}</span></button>`;
  return html`<button class="slide ${cls}" style="--g:${colour}" ${a}>${thumb(sl, d.ctx)}
    <div class="strip"><span class="n">${num}</span><i class="dot"></i><span class="lbl">${name}</span>${again ? html`<i class="sh-b" title="The same slide, sung again">again</i>` : ''}${badges}${d.song ? html`<span class="edit" data-a="editSlide:${d.key},${s}" title="Edit this slide">${I.pen}</span>` : ''}<span class="edit" data-a="menu:slide,${d.key},${s}" title="More">${I.more}</span></div></button>`;
}
function deckHTML(d) {
  const open = { id: `deck-${d.key}`, 'data-open': d.key === state.openItem ? 1 : 0 }, m = d.item ? { 'data-m': `item:${d.key}` } : null, at = html`<i class="sh-at" id="at-${d.key}"></i>`;
  if (d.kind === 'header') return html`${at}<div class="sh-head cap" ${open} ${m}>${d.title}</div>`;
  if (d.kind === 'missing') return html`${at}<div class="grp-h gone" ${open} ${m}><b>${d.title}</b><span class="pill">Missing</span><span class="sp"></span></div>`;
  if (d.kind === 'media') return html`${at}<div class="grp-h" ${open} ${m}><b>${d.title}</b><span class="pill">${d.media && d.media.video ? 'Video' : 'Picture'}</span><span class="sp"></span></div>
    <div class="grid"><button class="slide ${d.media && state.mediaOn === d.media.id ? 'live' : ''} ${d.media ? '' : 'off'}" ${d.media ? { 'data-a': `media:${d.media.id}` } : null}><div class="thumb"><div class="bg" style="background:${d.media ? d.media.bg : '#111'}"></div>${d.media && d.media.poster ? html`<img class="sl-img" src="${d.media.poster}" alt="" draggable="false">` : ''}${d.media && d.media.video ? html`<span class="vlen">${lenText(d.media.length || 0)}</span>` : ''}</div><div class="strip"><span class="lbl say">Click to show it</span></div></button></div>`;
  if (d.kind === 'mediaPlaylist') return html`${at}<div class="grp-h" ${open} ${m}><b>${d.title}</b><span class="pill">Media playlist</span><span class="sp"></span></div>
    <div class="grid"><button class="slide ${d.mp && state.mediaPl && state.mediaPl.id === d.mp.id ? 'live' : ''}" ${d.mp ? { 'data-a': `playMp:${d.mp.id}` } : null}><div class="thumb sh-stack">${d.mp ? d.mp.items.slice(0, 3).map((id, n) => html`<div class="bg" style="background:${(mediaById(id) || {}).bg};inset:${n * 7}% ${(2 - n) * 7}% ${(2 - n) * 7}% ${n * 7}%"></div>`) : ''}</div><div class="strip"><span class="lbl">${d.mp ? `${d.mp.items.length} items · click to play them round` : 'Gone'}</span></div></button></div>`;
  if (d.kind === 'placeholder') return html`${at}<div class="grp-h" ${open} ${m}><b>${d.title}</b><span class="pill">Placeholder</span><span class="sp"></span></div>
    <div class="grid"><button class="sh-slottile" data-a="fillSlot:${d.key}"><span>${I.plus} Choose a song…</span></button></div>`;
  const s = d.song, a = s && arrOf(s), showArr = s && (a || state.arrOpen[s.id]);
  let chips = '';
  if (showArr) {
    let q = 0; const liveQ = state.live && state.live.deck === d.key && state.on ? state.live.q : -1;
    chips = html`<div class="sh-arr" data-arr="${s.id}"><button class="pill sh-arrname" data-a="menu:arrPick,${s.id}">${a ? a.name : 'As written'} ▾</button>${!a ? html`<span class="sub">Pick or make an arrangement to sing the groups in another order</span>` : [arrRuns(a.groups).map(({ g, start: n, count }) => {
      const len = s.slides.filter(sl => sl.enabled && sl.group === g).length, on = liveQ >= q && liveQ < q + len * count, pass = on && len ? Math.floor((liveQ - q) / len) + 1 : 0; q += len * count;
      return html`<button class="sh-chip ${on ? 'on' : ''}" style="--g:${groupColour(g)}" data-a="arrGo:${d.key},${n}" data-m="arrChip:${s.id},${n}" draggable="true" data-drag="arr:${s.id},${n}" title="Click to go there · drag to move · the arrow sets how many times"><i class="dot"></i>${g}${count > 1 ? html`<b class="sh-x mono">${on ? `${pass}/${count}` : '×' + count}</b>` : ''}<span class="sh-chipmore" data-a="menu:arrChip,${s.id},${n}" title="Times, move or remove">${I.chev}</span></button>`; }), html`<button class="iconbtn" data-a="menu:arrAdd,${s.id}" title="Add a group">${I.plus}</button>`]}</div>`;
  }
  const catHere = d.key.startsWith('cat-');      // a catalog song: to look at and add, not to change
  const tools = catHere ? '' : (!s ? (d.app ? html`<span class="pill">Opens in the app</span>` : d.kind === 'passage' ? html`<span class="pill">Bible</span>${verTool(d)}` : d.kind === 'vox' ? html`<span class="pill">Live captions</span>` : '')
    : html`${d.slot ? html`<span class="pill">${d.slot}</span>` : ''}${s.loop ? html`<span class="pill">Loops</span>` : ''}<span class="sh-tools"><button class="iconbtn" data-a="songInfo:${s.id}" title="Details">${I.info}</button><button class="iconbtn" data-a="songWords:${s.id}" title="Edit Words">${I.words}</button><button class="iconbtn ${showArr ? 'lit' : ''}" data-a="songArr:${s.id}" title="Arrangement">${I.arr}</button>${verTool(d)}<button class="iconbtn ${s.notes ? 'lit' : ''}" data-a="songNotes:${s.id}" title="Notes">${I.notes}</button><button class="iconbtn" data-a="${d.item ? `menu:item,${d.key}` : `menu:song,${s.id}`}" title="More">${I.more}</button></span>`);
  const v = DOC.settings.view, wrap = body => html`<div class="${v === 'grid' ? 'grid' : v === 'easy' ? 'sh-list' : 'sh-tbl'}">${v === 'table' ? html`<div class="sh-thead"><span></span><span>Group</span><span>Words</span><span>Notes</span></div>` : ''}${body}</div>`;
  let body;
  if (a) {       // with an arrangement the slides stand in the order they are sung: a group sung again is shown again, and what is left out waits underneath
    const out = d.slides.map((_, i) => i).filter(i => !d.seq.includes(i));
    body = [wrap(d.seq.map((i, q) => slideTile(d, d.slides[i], i, { q, again: d.seq.indexOf(i) !== q, prev: q ? d.slides[d.seq[q - 1]].group : null }))),
      out.length ? [html`<div class="sh-outh cap">Not in this arrangement · still there to click</div>`, wrap(out.map((i, n) => slideTile(d, d.slides[i], i, { q: -1, out: true, prev: n ? d.slides[out[n - 1]].group : null })))] : ''];
  } else body = wrap(d.slides.map((sl, i) => slideTile(d, sl, i)));
  const askHere = s && state.changedOutside === s.id ? html`<div class="sh-ask"><i class="nk ask">?</i><span>This song was changed outside SoftPresenter. Read it again from its file, or keep SoftPresenter’s version?</span><button class="btn sm primary" data-a="outside:reload">Reload</button><button class="btn sm" data-a="outside:keep">Keep mine</button></div>` : '';
  return html`${at}<div class="grp-h" ${open} ${m}><b>${d.title}</b>${tools}<span class="sp"></span></div>${catHere ? catBar(d) : catLine(s)}${askHere}${chips}${body}`;
}
function renderShowCentre() {
  const st = DOC.settings, tr = st.transition, mv = state.mediaView && byId(DOC.mediaPlaylists, state.mediaView);
  $('media').hidden = false; $('foot').hidden = false;
  $('slides').className = 'slides';
  document.documentElement.style.setProperty('--tile', st.size + 'px');
  const ds = decks();
  put('slides', ds.length ? ds.map(deckHTML) : state.src.type === 'pl'
    ? html`<div class="sh-empty"><b>${playlist().name} is empty</b><span>A playlist is the running order of a service: songs, Bible passages, pictures and videos, in the order they come.</span>
        <div><button class="btn primary" data-a="emptyAdd:song">${I.plus} Add a song</button><button class="btn" data-a="emptyAdd:passage">Add a Bible passage</button><button class="btn" data-a="emptyAdd:header">Add a header</button><button class="btn" data-a="importSheet">Import…</button></div></div>`
    : html`<div class="sh-empty"><b>No song is open</b><span>${librarySongs().length ? 'Pick a song on the left to see its slides.' : 'This library has no songs yet.'}</span><div><button class="btn primary" data-a="newSong">${I.plus} New song</button><button class="btn" data-a="importSheet">Import…</button></div></div>`);
  put('foot', html`<div class="seg"><button class="${tr.kind === 'Cut' ? 'on' : ''}" data-a="trans:Cut">Cut</button><button class="${tr.kind === 'Dissolve' ? 'on' : ''}" data-a="trans:Dissolve">Dissolve</button>
      <button class="${tr.kind !== 'Cut' && tr.kind !== 'Dissolve' ? 'on' : ''}" data-a="transSheet" title="Every other transition"><span class="sh-trk">${tr.kind !== 'Cut' && tr.kind !== 'Dissolve' ? tr.kind : 'Advanced'}</span> ▾</button></div>
    <i class="sh-div"></i><button class="btn ${st.continuous ? 'onkey' : ''}" data-a="continuous" title="Next at the end of a song goes on to the next song">Continuous</button>
    ${DOC.versions.length ? html`<button class="btn sh-vbtn ${state.version ? 'lit' : ''}" data-a="menu:version" title="Theme version: ${state.version ? `the ${versionName(state.version.id)} version is on` : 'the usual look'}. Choose a version, such as ${DOC.versions[0].name}, for every screen whose theme has it" aria-label="Theme version">${I.theme}<i class="chev">▾</i></button>` : ''}
    <i class="sh-div sh-urdiv"></i><button class="iconbtn ui-ur" data-a="undo" aria-label="Undo" ${undoStack.length ? { title: `Undo (${keyText('⌘Z')})` } : { disabled: true, title: 'Nothing to undo' }}>${I.undo}</button><button class="iconbtn ui-ur" data-a="redo" aria-label="Redo" ${redoStack.length ? { title: `Redo (${keyText('⇧⌘Z')})` } : { disabled: true, title: 'Nothing to redo' }}>${I.redo}</button><span class="sp"></span>
    <div class="seg">${[['grid', I.grid, 'Grid'], ['easy', I.easy, 'Words only'], ['table', I.table, 'Table']].map(([k, ic, t]) => html`<button class="${st.view === k ? 'on' : ''}" data-a="view:${k}" title="${t}">${ic}</button>`)}</div>
    <input type="range" min="130" max="340" value="${st.size}" data-i="size" aria-label="Slide size">`);
  const list = (mv ? mv.items.map(mediaById).filter(Boolean) : DOC.media).filter(m => !m.deck), mt = DOC.mediaTransition;
  put('media', html`<div class="media-h"><span class="cap">Media</span><kbd class="kh">${keyText('⇧⌘M')} then 1–9</kbd><button class="pill" data-a="menu:mediaSrc">${mv ? mv.name : 'All Media'} ▾</button>
      ${mv ? html`<button class="btn sm" data-a="playMp:${mv.id}">${I.play} Play this playlist</button>${mv.follow ? html`<span class="pill">Follows ${mv.follow}</span>` : ''}` : ''}
      <span class="sp"></span><button class="pill" data-a="menu:mediaTr">${mt.kind}${mt.kind === 'Cut' ? '' : ' ' + mt.time + 's'} ▾</button><button class="iconbtn" data-a="addMedia" title="Add pictures or videos">${I.plus}</button><button class="iconbtn sh-mhide ${state.mediaHidden ? 'up' : ''}" data-a="mediaHide" title="${state.mediaHidden ? 'Show the media strip' : 'Hide the media strip'}" aria-label="${state.mediaHidden ? 'Show the media strip' : 'Hide the media strip'}">${I.chev}</button></div>
    <div class="bin" ${{ hidden: !!state.mediaHidden }}>${list.length ? list.map(m => html`<button class="clip ${state.mediaOn === m.id ? 'on' : ''} ${state.mediaPick.includes(m.id) ? 'pickd' : ''}" data-a="media:${m.id}" data-m="media:${m.id}" draggable="true" data-drag="media:${m.id}">
      <div class="pic" style="background:${m.bg}">${m.poster ? html`<img class="sl-img" src="${m.poster}" alt="" draggable="false">` : ''}${m.video ? html`<span class="vlen">${lenText(m.length || 0)}</span>` : ''}<span class="sh-clipmore" data-a="menu:media,${m.id}" title="More">${I.more}</span></div><div class="nm" title="${m.name}">${mediaLabel(m)}</div></button>`) : html`<button class="sh-binempty" data-a="addMedia">${I.plus} ${mv ? 'This media playlist is empty. Add pictures or videos' : 'Add pictures or videos to show behind the words'}</button>${!mv && samplesMissing().length ? html`<button class="sh-binempty sh-binsample" data-a="sampleBackgrounds">Or use the sample backgrounds</button>` : ''}`}</div>`);
}
A.trans = ([kind]) => mutate(() => { DOC.settings.transition = { kind, time: kind === 'Cut' ? 0 : .5, dir: 'Left' }; });
A.transSheet = () => { const t = DOC.settings.transition; openSheet({ title: 'Transition for all slides',
  body: html`${field('Kind', html`<select class="sel" id="trKind">${options(TRANSITIONS, t.kind)}</select>`)}${field('Time', html`<input type="range" id="trTime" min="0" max="5" step="0.1" value="${t.time}" style="width:100%">`)}${field('Direction', html`<select class="sel" id="trDir">${options(['Left', 'Right', 'Up', 'Down'], t.dir)}</select>`)}`,
  note: 'A song and a slide can each have their own, from their menus', buttons: [{ t: 'Cancel' }, { t: 'Use it', primary: true, f: () => mutate(() => { DOC.settings.transition = { kind: $('trKind').value, time: +$('trTime').value, dir: $('trDir').value }; }) }] }); };
A.continuous = () => mutate(() => { DOC.settings.continuous = !DOC.settings.continuous; });
A.view = ([v]) => { DOC.settings.view = v; render(); };

/* ───────── media bin ───────── */
A.media = ([id], el, ev) => { if (ev && (ev.metaKey || ev.ctrlKey || ev.shiftKey)) { state.mediaPick.includes(id) ? state.mediaPick.splice(state.mediaPick.indexOf(id), 1) : state.mediaPick.push(id); return render(); } state.mediaPick = [];
  if (state.ann && state.ann.media === id && !state.annOffSlides) return;      // already showing over the slides: nothing changes
  if (state.annLast === id) return playAnnMedia(id);      // what played on the announcements layer plays there again, every time
  showMedia(id); };
A.playMp = ([id]) => playMediaPlaylist(id);
A.addMedia = () => pickFiles('image/*,video/*', addMediaFiles);
// The media strip folds away to its heading and back (it was View › Media Bin in the Mac menu); ⇧⌘M opens it again to number what is in it
A.mediaHide = () => { state.mediaHidden = !state.mediaHidden; render(); };
// The sample backgrounds that come with the app (CC0, drawn for it; design-requests/backgrounds): added to the media bin as any
// picture is, and gathered in their own media playlist so they are easy to find and to remove together. One already in the
// bin is not added twice; once all are in, the media playlist itself is how to reach them.
const SAMPLE_BACKGROUNDS = [['Still.jpg', 'still'], ['First Light.jpg', 'first-light'], ['Night Blue.jpg', 'night-blue'], ['Evening Hills.jpg', 'evening-hills'],
  ['Olive.jpg', 'olive'], ['Room Out of Focus.jpg', 'room-out-of-focus'], ['Drifting Light.mp4', 'drifting-light', 16]];
const samplesMissing = () => SAMPLE_BACKGROUNDS.filter(([name]) => !DOC.media.some(m => m.sample === name));
A.sampleBackgrounds = () => {
  const add = samplesMissing(), mp = () => DOC.mediaPlaylists.find(p => p.sample);
  if (!add.length) { if (mp()) { state.mediaView = mp().id; render(); } return; }
  mutate(() => {
    const made = add.map(([name, file, length]) => ({ id: newId('m'), name, sample: name, bg: `url("samples/${file}.jpg") center / cover`, scaling: 'Scale to Fill', ...(length ? { video: true, length, loop: true } : {}) }));
    DOC.media.push(...made);
    if (!mp()) DOC.mediaPlaylists.push({ id: newId('mp'), name: 'Sample Backgrounds', items: [], follow: null, sample: true });
    const p = mp(); p.items.push(...made.map(m => m.id)); state.mediaView = p.id;
  });
};
M.media = ([id]) => { const m = mediaById(id), d = decks().find(x => x.key === state.openItem && x.song) || decks().find(x => x.song); return [
  { t: 'Show', f: () => showMedia(id) },
  { t: 'Play on Announcements Layer', f: () => playAnnMedia(id) },
  { t: 'Scaling', sub: ['Scale to Fill', 'Scale to Fit', 'Stretch to Fill'].map(s => ({ t: s, on: m.scaling === s, f: () => mutate(() => { m.scaling = s; }) })) },
  m.video && { t: 'Loop', on: m.loop, f: () => mutate(() => { m.loop = !m.loop; }) },
  state.src.type === 'pl' && { t: `Add to Playlist “${playlist().name}”`, f: () => addItem({ kind: 'media', media: id }) },
  d && { t: `Add to “${d.song.title}” as a Slide`, f: () => mutate(() => { d.song.slides.push(slide('', '', '', { media: id, label: m.name, objects: [] })); }) },
  { t: 'Media Playlists', sub: DOC.mediaPlaylists.map(p => ({ t: p.name, on: p.items.includes(id), f: () => mutate(() => { p.items.includes(id) ? p.items.splice(p.items.indexOf(id), 1) : p.items.push(id); }) })) },
  { sep: true }, state.mediaView ? (() => { const mp = byId(DOC.mediaPlaylists, state.mediaView), all = state.mediaPick.includes(id) ? state.mediaPick : [id]; return mp.follow ? { t: `Remove from “${mp.name}”`, dim: true, f: () => refuse('It follows a folder: what is in the folder is in it, by name') } : { t: all.length > 1 ? `Remove ${all.length} Items from “${mp.name}”` : `Remove from “${mp.name}”`, f: () => mutate(() => { mp.items = mp.items.filter(x => !all.includes(x)); state.mediaPick = []; }) }; })() : { t: 'Delete…', f: () => confirmDo(`Delete “${m.name}” from the media bin?`, 'The file stays where it is on the computer. This can’t be undone.', 'Delete', () => mutate(() => { DOC.media.splice(DOC.media.indexOf(m), 1); DOC.mediaPlaylists.forEach(p => { p.items = p.items.filter(x => x !== id); }); if (state.mediaOn === id) state.mediaOn = null; })) }]; };
M.mediaSrc = () => { const mv = state.mediaView && byId(DOC.mediaPlaylists, state.mediaView); return [
  { t: 'All Media', on: !mv, f: () => { state.mediaView = null; render(); } }, ...DOC.mediaPlaylists.map(p => ({ t: p.name, on: mv === p, f: () => { state.mediaView = p.id; render(); } })), { sep: true },
  { t: 'New Media Playlist…', f: () => { const p = { id: newId('mp'), name: 'New Media Playlist', items: [], follow: null }; mutate(() => { DOC.mediaPlaylists.push(p); state.mediaView = p.id; }); renameAt('[data-a="menu:mediaSrc"] span', p.name, v => mutate(() => { p.name = v; }), 'Name the media playlist'); } },
  samplesMissing().length && { t: 'Add the Sample Backgrounds', f: () => A.sampleBackgrounds() },
  mv && { t: 'Play This Playlist', f: () => playMediaPlaylist(mv.id) },
  mv && (mv.follow ? { t: 'Stop Following the Folder', f: () => mutate(() => { mv.follow = null; }) } : { t: 'Follow a Folder…', f: () => ask('Follow a folder', 'Folder', 'Announcements export', v => mutate(() => { mv.follow = v; }), { note: 'Pictures and videos put in it join the playlist as they appear' }) }),
  mv && { t: 'Rename…', f: () => ask('Rename', 'Name', mv.name, v => mutate(() => { mv.name = v; })) },
  mv && { t: 'Delete', f: () => mutate(() => { DOC.mediaPlaylists.splice(DOC.mediaPlaylists.indexOf(mv), 1); state.mediaView = null; }) }]; };
M.mediaTr = () => [['Cut', 0], ['Dissolve', .4], ['Dissolve', 1], ['Fade', 1]].map(([kind, time]) => ({ t: kind === 'Cut' ? 'Cut' : `${kind} ${time}s`, on: DOC.mediaTransition.kind === kind && DOC.mediaTransition.time === time, f: () => mutate(() => { DOC.mediaTransition = { kind, time }; }) }));

/* ───────── bringing songs in and sending them out (the file part is the app's; this shows what happens) ───────── */
M.importMenu = () => [{ head: 'Import Songs' }, (APPLE ? { t: 'A Library Folder…', f: () => A.importDo(['folder']) } : { t: 'A Library Folder…', sub: [{ t: 'A Folder…', f: () => A.importDo(['folder']) }, { t: 'Files…', f: () => A.importDo(['folder']) }] })      /* Windows' and Linux's pickers choose a folder or files, not both */, { t: 'A Song Bundle…', f: () => A.importDo(['pack']) }, { t: 'Song Files…', f: () => A.importDo(['songs']) }, { t: 'Words from Text…', f: () => A.newSong() },
  { sep: true }, { t: 'Slides…', f: () => A.importSheet() }];
A.importSheet = () => openSheet({ title: 'Import Slides', wide: true,
  body: html`<div class="ui-cards">
    <button class="ui-card" data-a="importDo:pictures"><b>Pictures as slides</b><span>Pictures chosen together become one presentation, a slide each, in the order their names number them</span></button>
    <button class="ui-card" data-a="importPres"><b>A presentation</b><span>A .pptx, .ppsx, .key, .odp, .ppt or .pps file, or a PDF. You choose how it comes in</span></button></div>`,
  note: 'In the app you can also drop files on the window', buttons: [{ t: 'Close' }] });
// A presentation (as the app has it, c46ddff): As Slides (editable, read straight from a .pptx or .ppsx) or As Pictures (the exact
// look, drawn unseen by an app on this computer that can open the file). The last choice for each kind of file is ready on Enter.
const PRES_FILES = '.pptx,.ppsx,.key,.odp,.ppt,.pps,.pdf';
A.importPres = () => pickFiles(PRES_FILES, files => files[0] && presChoice(files[0].name));
function presChoice(name) {
  closeSheet();
  const ext = (name.split('.').pop() || '').toLowerCase();
  if (ext === 'pdf') return presImport(name, 'pictures');      // a PDF is pictures already: no question
  const slides = ext === 'pptx' || ext === 'ppsx', pictures = !state.noDrawApp, kept = (DOC.settings.importAs || {})[ext], last = kept || (slides ? 'slides' : 'pictures');
  if (!slides && !pictures) return openSheet({ title: `Import “${name}”`, body: html`<p class="ui-p">No app on this computer can open .${ext} files to draw them, so “${name}” can’t come in. Saved as .pptx, it could come in as slides.</p>`, buttons: [{ t: 'OK', primary: true }] });
  const pick = slides && pictures ? last : slides ? 'slides' : 'pictures';
  const card = (how, title, line, can, why) => html`<button class="ui-card ui-choice ${pick === how ? 'on' : ''}" data-a="presAs:${how}" ${can ? null : { disabled: true, title: why }}>
    <b>${title}${pick === how && kept === how && slides && pictures ? html`<i class="ui-last cap">Last time</i>` : ''}</b><span>${can ? line : why}</span></button>`;
  state.presFile = { name, ext, pick };
  openSheet({ title: `Import “${name}”`, wide: true, body: html`<div class="ui-cards">
    ${card('slides', 'As Slides', 'Words stay words, ready to edit. Read straight from the file in a moment. Charts and tables come in as pictures', slides, `Only .pptx and .ppsx files can come in as slides`)}
    ${card('pictures', 'As Pictures', 'The exact look, each slide a picture. An app on this computer that can open the file draws it unseen, and is closed again', pictures, `No app on this computer can open .${ext} files to draw them`)}</div>`,
    note: `Enter takes the one ringed. The choice is kept for .${ext} files`, buttons: [{ t: 'Cancel' }] });
}
A.presAs = ([how]) => { const f = state.presFile; if (!f) return; DOC.settings.importAs = { ...(DOC.settings.importAs || {}), [f.ext]: how }; closeSheet(); presImport(f.name, how); };
// The sample: five slides. As Slides, two are named to check (a chart and a table that came in as pictures), one keeps its notes, one
// keeps its words' own sizes and colours (a .pptx's runs become styled words), and one hidden in the file comes in switched off.
// A PDF's deck shows at once and its pages fill in.
function presSlide(p, n) {
  const sl = slide('', `${p}\n${n === 1 ? '• Youth evening, Friday\n• Lunch after the service' : ''}`.trim(), '', { label: '', notes: n === 0 ? 'Welcome everyone; mention the visitors’ card' : '', enabled: n !== 3, check: n === 2 ? 'A chart came in as its picture' : n === 4 ? 'A table came in as its picture' : '' });
  if (n === 1) { const m = sl.objects.find(o => o.role === 'main'); m.spans = [{ from: 0, to: p.length, size: 120, color: '#8fc8ff' }, { from: m.text.indexOf('Friday'), to: m.text.indexOf('Friday') + 6, italic: true, bold: false }]; }      // a heading bigger and coloured, a day in italics
  return sl;
}
function presImport(name, how) {
  const title = name.replace(/\.[^.]+$/, ''), pages = ['Welcome', 'This week', 'Giving', 'Small groups', 'Thank you'];
  let s;
  mutate(() => {
    s = song(title, []);
    s.slides = pages.map((p, n) => how === 'slides'
      ? presSlide(p, n)
      : slide('', '', '', { label: `Page ${n + 1}`, pending: name.toLowerCase().endsWith('.pdf'), objects: [obj('image', `Page ${n + 1}`, { media: DOC.media[n % 3].id, frame: { x: 0, y: 0, w: 1920, h: 1080 } })] }));
    DOC.songs.push(s); state.src = { type: 'lib', id: 'all' }; state.openSong = s.id;
  });
  const checks = s.slides.map((x, n) => x.check ? n + 1 : 0).filter(Boolean);
  const named = checks.length > 8 ? `${checks.slice(0, 8).join(', ')} and ${checks.length - 8} more` : checks.length > 1 ? `${checks.slice(0, -1).join(', ')} and ${checks[checks.length - 1]}` : String(checks[0]);
  const reasons = [...new Set(s.slides.filter(x => x.check).map(x => x.check.toLowerCase()))].join('; ');      // each reason once
  toast(how === 'slides' ? `“${title}” came in as ${s.slides.length} slides.${checks.length ? ` Check slide${checks.length > 1 ? 's' : ''} ${named}: ${reasons}.` : ''}` : `“${title}” came in as ${s.slides.length} picture slides.`);
  if (s.slides.some(x => x.pending)) { let n = 0; const fill = () => { const sl = s.slides[n++]; if (!sl) return; delete sl.pending; render(); setTimeout(fill, 150); }; setTimeout(fill, 150); }
}
A.importDo = ([what]) => {
  closeSheet();
  if (what === 'songs') return pickFiles('.txt,.text,.cho,.chopro,.crd,.pro,text/plain', files => Promise.all(files.map(f => f.text().then(t => importSongText(f.name, t)))).then(addImportedSongs));
  if (what === 'pictures') return pickFiles('image/*', addPictureSlides);
  if (what.startsWith('sample-')) what = what.slice(7);
  mutate(() => {
    if (what === 'slides') {      // the old sample, kept for the checks: a PDF's three pages
      const s = song('Sermon slides.pdf', []); s.slides = [1, 2, 3].map(n => slide('', '', '', { label: `Page ${n}`, objects: [obj('image', `Page ${n}`, { media: DOC.media[(n - 1) % 3].id, frame: { x: 0, y: 0, w: 1920, h: 1080 } })] }));
      DOC.songs.push(s); state.src = { type: 'lib', id: 'all' }; state.openSong = s.id; return toast('3 pages came in as picture slides');
    }
    const made = song(what === 'pack' ? 'Lantern Song' : 'River Song', [['Verse 1', ['Down by the river\nWe sing', '']], ['Chorus', ['Flow on, flow on', '']], ['Verse 2', ['Up on the hill', '']]]);
    made.info.author = 'Anna Writer'; DOC.songs.push(made);
    if (what === 'folder') { DOC.libraries.push({ id: newId('lib'), name: 'Imported library', songs: [made.id] }); state.src = { type: 'lib', id: DOC.libraries[DOC.libraries.length - 1].id }; }
    else state.src = { type: 'lib', id: 'all' };
    state.openSong = made.id;
    toast(what === 'pack' ? '1 song, 1 theme and 2 pictures came in from the pack' : `1 song came in${what === 'songs' ? '' : ', in a library of its own'}`);
  });
};
function exportSheet(name, songIds) {
  openSheet({ title: `Export “${name}”`, body: html`<p class="ui-p">A song pack with ${songIds.length} song${songIds.length === 1 ? '' : 's'}, the themes they set for themselves, and their pictures and videos. Nothing from this computer's paths or user name goes in.</p>
    <label class="ui-check"><input type="checkbox" id="exVideos" checked> Include videos (about 48 MB)</label>`,
    buttons: [{ t: 'Cancel' }, { t: 'Export', primary: true, f: () => toast(`${name}.sppack was made${$('exVideos').checked ? '' : ' without its videos'}`) }] });
}

/* ═════════ live.js ═════════ */
// The right column: what is on screen, the layers and their clears, and the live tools. It is the same in every mode.
'use strict';

const stageScreens = () => DOC.screens.filter(s => s.kind === 'Stage');
// The preview shows the audience picture or the stage picture; from its small menu it can show any one screen as that screen draws it.
const monScreen = () => state.monScreen && byId(DOC.screens, state.monScreen) || null;
function monitorHTML() {
  const one = monScreen();
  if (one) { const off = one.disconnected ? 'Not connected' : !one.on ? 'This screen is off' : !(one.kind === 'Stage' ? state.stage : state.audience) ? (one.kind === 'Stage' ? 'The stage screens are off' : 'The audience screens are off') : '';
    return html`${one.kind === 'Stage' ? stageHTML(one.layout) : screenHTML(one)}${off ? html`<div class="off">${off}</div>` : ''}`; }
  if (state.monitor === 'Stage') { const sc = stageScreens()[0]; return html`${stageHTML(sc && sc.layout)}${state.stage ? '' : html`<div class="off">The stage screens are off</div>`}`; }
  return html`${screenHTML(null)}${state.audience ? '' : html`<div class="off">The audience screens are off</div>`}`;
}
// The preview is redrawn only when its picture really changes, so a transition, a build, a ticker or a video keeps moving in it.
// The new picture is made apart and compared with the last one as it was made (before fitAll sized its words).
let monLast = null, monStamp = 0, monBuild = 0;
function renderMonitor() {
  const box = $('monLayers'); if (!box) return;
  const pic = put(document.createElement('div'), monitorHTML());
  if (!monLast || !pic.isEqualNode(monLast)) { monLast = pic.cloneNode(true); box.replaceChildren(...pic.childNodes); fitAll(box); }
  syncVideo();
  const on = state.on; if (state.monitor !== 'Audience' || !on) { monBuild = state.build; return; }
  if (on.stamp !== monStamp) {        // a slide just went up: its transition, and the builds that come with it
    monStamp = on.stamp; monBuild = state.build;
    const tr = on.tr, sl = box.querySelector('.lv-slide');
    if (sl && tr && tr.kind !== 'Cut') { const d = { Left: [1, 0], Right: [-1, 0], Up: [0, 1], Down: [0, -1] }[tr.dir] || [1, 0];
      sl.style.setProperty('--t', (tr.time || .5) + 's'); sl.style.setProperty('--dx', d[0]); sl.style.setProperty('--dy', d[1]); sl.classList.add('tr', 'tr-' + tr.kind.toLowerCase().replace(/ /g, '-')); }
    box.querySelectorAll('[data-bd]').forEach(el => { const [kind, from, delay] = el.dataset.bd.split(','); el.style.animationDelay = delay + 's'; el.classList.add('bd', 'bd-' + kind, 'from-' + from); });
  } else if (state.build !== monBuild) {      // a click build
    const el = state.build > monBuild && box.querySelector(`[data-bdstep="${state.build - 1}"]`);
    if (el) { const [kind, from] = el.dataset.bdk.split(','); el.classList.add('bd', 'bd-' + kind, 'from-' + from); }
    monBuild = state.build;
  }
}
// A video added from a real file plays in the preview; the controls drive it.
function syncVideo() {
  const el = $('monVideo'), m = videoOn(), real = m && m.url && state.monitor === 'Audience' && (!monScreen() || monScreen().layers.includes('media'));
  el.hidden = !real;
  if (!real) { if (!el.paused) el.pause(); return; }
  if (el.dataset.id !== m.id) { el.dataset.id = m.id; el.src = m.url; el.currentTime = state.video.pos || 0; }
  el.loop = state.video.loop && !state.mediaPl; el.muted = state.video.mute; el.volume = DOC.settings.sound.video.level / 100;
  if (Math.abs(el.currentTime - state.video.pos) > 1.5) el.currentTime = state.video.pos;
  if (state.video.playing && el.paused) el.play().catch(() => {}); if (!state.video.playing && !el.paused) el.pause();
}

function renderRight() {
  const on = anythingOn(), s = state.on, rv = rowVideo(), v = rv ? rv.v : state.video, vm = rv && rv.m, vk = rv ? rv.key : 'video', m = state.mediaOn && mediaById(state.mediaOn);
  // In the Edit view the column is the inspector alone, from the top (the owner, 25 November: "Remove it entirely"; the app, e4713a6)
  document.querySelector('.right').classList.toggle('editing', state.mode === 'edit' || state.mode === 'themes');
  $('program').className = 'program' + (on ? ' on' : '');
  const mp = state.mediaPl && byId(DOC.mediaPlaylists, state.mediaPl.id);
  put('pgHead', html`<div class="pg-h"><i class="dot"></i>${seg(['Audience', 'Stage'], state.monitor, 'monitor')}${DOC.screens.length > 2 ? html`<button class="iconbtn lv-monpick ${monScreen() ? 'lit' : ''}" data-a="menu:monPick" title="Look at one screen" aria-label="Look at one screen"><span class="sc-chev">${I.chev}</span></button>` : ''}<span class="state">${on ? 'ON SCREEN' : 'CLEAR'}</span></div>`);
  renderMonitor();
  put('pgFoot', html`<div class="readout" ${s ? { 'data-a': 'goLiveSlide', title: 'Go to the slide on screen', role: 'button', tabindex: '0' } : {}}><span class="big">${s ? two(s.n) : '--'}<small>/${s ? two(s.total) : '--'}</small></span>
      <span class="txtline"><span class="l1">${s ? html`<b>${s.group || ''}</b> · ${s.title}` : state.ann && annName() ? html`<b>Announcements</b> · ${annName()}` : 'Nothing on screen'}</span><span class="l2">${[monScreen() ? 'Looking at ' + monScreen().name : '', s && buildSteps(s.slide).length ? `build ${state.build}/${buildSteps(s.slide).length}` : ''].filter(Boolean).join(' · ')}${state.advanceLeft ? html`${monScreen() || (s && buildSteps(s.slide).length) ? ' · ' : ''}next in <span id="lvAdv">${state.advanceLeft}</span>s` : ''}</span></span></div>
    <div class="pg-ctx">${vm ? html`<div class="lv-vid"><button class="vk big" data-a="${vk}:play" title="${v.playing ? 'Pause' : 'Play'}" aria-label="${v.playing ? 'Pause' : 'Play'}">${v.playing ? I.pause : I.play}</button><button class="vk" data-a="${vk}:restart" title="Start over">${I.prev}</button><button class="vk" data-a="${vk}:fwd" title="Forward 10 seconds">${I.fwd}</button>
        <span class="tm" id="vGone" hidden>${mmss(v.pos)}</span><input type="range" id="vSeek" data-i="vseek" min="0" max="${vm.length}" value="${v.pos}" aria-label="Place in the video"><span class="tm" id="vLeft">-${mmss(vm.length - v.pos)}</span>
        <button class="vk ${v.loop ? 'act' : ''}" data-a="${vk}:loop" title="Loop">${I.loop}</button><button class="vk ${v.mute ? '' : 'act'}" data-a="${vk}:mute" title="Sound">${v.mute ? I.mute : I.sound}</button>${mp ? html`<button class="vk wide" data-a="clear:media" title="Stop the media playlist ${mp.name}">Stop</button>` : ''}</div>`
      : mp ? html`<div class="lv-line"><span>Playing <b>${mp.name}</b> · ${state.mediaPl.n + 1} of ${mp.items.length}</span><button class="vk wide" data-a="clear:media">Stop</button></div>`
      : state.test !== 'off' ? html`<div class="lv-line"><span>Test pattern: <b>${({ bars: 'Colour bars', grid: 'Grid', white: 'White' })[state.test]}</b></span><button class="vk wide" data-a="test:off">Off</button></div>`
      : state.identify ? html`<div class="lv-line"><span>Identifying the screens: <b>the room sees it</b></span><button class="vk wide" data-a="identify" title="Take each screen’s name and colour off it">Stop</button></div>` : ''}</div>`);      // Identify can be stopped from anywhere

  // in the order the screens stack them, from the back (the app, 58a7d95); audio is heard, not seen, so it comes last
  const keys = [['media', 'Media'], ['slide', 'Slide'], ['announcements', 'Announce'], ['props', 'Props'], ['messages', 'Messages'], ['audio', 'Audio']];
  put('layers', keys.map(([k, nm]) => html`<button class="layer ${layerOn(k) ? 'on' : ''}" data-a="clear:${k}" title="${layerOn(k) ? 'Clear it' : 'Nothing showing'}"><i class="led"></i><span class="nm">${nm}</span></button>`),
    html`<button class="clearall ${on ? '' : 'idle'}" data-a="clear:all">Clear All</button>`);
  put('transport', html`<button class="tbtn" data-a="step:-1">${I.prev} Previous</button><button class="tbtn nextbtn" data-a="step:1">Next ${I.next}</button>`);
  put('mobilebar', html`<button class="tbtn" data-a="step:-1">${I.prev} Prev</button><button class="tbtn clr ${on ? '' : 'idle'}" data-a="clear:all">Clear All</button><button class="tbtn nextbtn" data-a="step:1">Next ${I.next}</button>`);

  if (state.mode === 'edit' || state.mode === 'themes') return renderInspector();
  const tabs = ['Timers', 'Messages', 'Props', 'Macros', 'Stage', 'Audio', ...(voxReady() ? ['VoxLive'] : [])];
  if (!tabs.includes(state.tab)) state.tab = 'Timers';
  const dots = { Timers: DOC.timers.some(t => t.run), Messages: !!state.message, Props: state.propsOn.length > 0, Audio: state.audio.playing, VoxLive: !!state.vox };
  $('tabs').className = 'tabs lv-tabs';
  put('tabs', tabs.map((t, n) => html`<button class="${t === state.tab ? 'on' : ''}" data-a="tab:${t}" title="${t} (${keyText('⌥' + (n + 1))}, then 1 to 9 for what is in it)">${t}${dots[t] ? html`<i></i>` : ''}<kbd class="kh">${keyText('⌥' + (n + 1))}</kbd></button>`));
  put('tabbody', TAB[state.tab]());
  fitAll($('tabbody'));
}
A.monitor = ([w]) => { state.monitor = w; state.monScreen = null; renderRight(); };
M.monPick = () => { const pick = sc => ({ t: sc.name + (sc.disconnected ? ' · not connected' : !sc.on ? ' · off' : ''), on: state.monScreen === sc.id, f: () => { state.monScreen = sc.id; state.monitor = sc.kind === 'Stage' ? 'Stage' : 'Audience'; renderRight(); } });
  const au = DOC.screens.filter(s => s.kind !== 'Stage'), sg = stageScreens();
  return [{ head: 'Audience' }, { t: 'What the room sees', on: !state.monScreen && state.monitor === 'Audience', f: () => A.monitor(['Audience']) }, ...au.map(pick), { sep: true }, { head: 'Stage' }, ...(sg.length > 1 ? [{ t: 'The first stage screen', on: !state.monScreen && state.monitor === 'Stage', f: () => A.monitor(['Stage']) }] : []), ...sg.map(pick)]; };
A.tab = ([t]) => { state.tab = t; renderRight(); };
A.clear = ([k]) => clear(k);
A.step = ([d]) => step(+d);
A.test = ([k]) => { state.test = k; render(); };
A.toScreens = () => { state.mode = 'settings'; state.section = 'Screens'; render(); };      // where Option-Command-1, “Set up screens…” and the Screens menu now land
A.annVideo = ([what]) => { const r = rowVideo(); if (r && r.key === 'annVideo') videoDo(r.v, r.m, what); renderRight(); };      // a video announcement's own player
const videoDo = (v, m, what) => { if (what === 'play') { if (!v.playing && v.pos >= m.length) v.pos = 0; v.playing = !v.playing; }
  if (what === 'restart') v.pos = 0; if (what === 'fwd') v.pos = Math.min(m.length, v.pos + 10); if (what === 'loop' || what === 'mute') v[what] = !v[what]; };
A.video = ([what]) => { const v = state.video, m = videoOn(); if (!m) return;
  if (what === 'play') { if (!v.playing && v.pos >= m.length) v.pos = 0; v.playing = !v.playing; }
  if (what === 'restart') v.pos = 0; if (what === 'fwd') v.pos = Math.min(m.length, v.pos + 10);
  if (what === 'loop' || what === 'mute') v[what] = !v[what];
  syncVideo(); renderRight(); };

/* ───────── the tabs ───────── */
const TAB = {};
const head = (label, action, extra = '') => html`<div class="lv-h"><span class="cap">${label}</span>${extra}<button class="iconbtn" data-a="${action}" title="New">${I.plus}</button></div>`;
const more = (kind, id) => html`<button class="iconbtn lv-more" data-a="menu:${kind},${id}" aria-label="More">${I.more}</button>`;

const timerSays = t => t.kind === 'down' ? `Counts down from ${fmt(t.start).replace(/^00:/, '')}` : t.kind === 'to' ? `Counts down to ${t.at}` : 'Counts up';
TAB.Timers = () => html`${head('Timers', 'timerEdit')}${DOC.timers.map(t => html`<div class="card ${t.run ? 'run' : ''}"><div class="t" title="${timerSays(t)}"><b>${t.name}</b></div>
  <span class="time mono" data-time="${t.id}">${fmt(t.left)}</span><button class="iconbtn" data-a="timer:reset,${t.id}" aria-label="Reset">${I.reset}</button><button class="iconbtn" data-a="timer:run,${t.id}" aria-label="Start or stop">${t.run ? I.pause : I.play}</button>${more('timer', t.id)}</div>`)}`;
A.timer = ([what, id]) => { const t = byId(DOC.timers, id); if (what === 'run') t.run = !t.run; else { t.left = t.kind === 'down' ? t.start : t.kind === 'to' ? secondsTo(t.at) : 0; t.run = false; } renderRight(); };
const secondsTo = at => { const [h, m] = at.split(':').map(Number), now = new Date(), then = new Date(now); then.setHours(h, m, 0, 0); return Math.max(0, Math.round((then - now) / 1000)); };
A.timerEdit = ([id]) => { const t = id ? byId(DOC.timers, id) : { name: '', kind: 'down', start: 300, at: '10:30' };
  openSheet({ title: id ? 'Timer' : 'New timer', body: html`${field('Name', html`<input class="field" id="tmName" value="${t.name}" placeholder="Sermon">`)}${
      field('Kind', html`<select class="sel" id="tmKind">${options([['down', 'Count down'], ['to', 'Count down to a time of day'], ['up', 'Count up']], t.kind)}</select>`)}${
      field('From (minutes:seconds)', html`<input class="field" id="tmStart" value="${mmss(t.start || 0)}">`)}${field('To a time of day', html`<input class="field" id="tmAt" type="time" value="${t.at || '10:30'}">`)}`,
    buttons: [{ t: 'Cancel' }, { t: 'Save', primary: true, f: () => { const name = $('tmName').value.trim(); if (!name) return false;
      mutate(() => { const [mm, ss] = $('tmStart').value.split(':').map(Number), kind = $('tmKind').value, start = (mm || 0) * 60 + (ss || 0), at = $('tmAt').value || '10:30';
        const data = { name, kind, start, at, left: kind === 'down' ? start : kind === 'to' ? secondsTo(at) : 0, run: false };
        if (id) Object.assign(t, data); else DOC.timers.push({ id: newId('tm'), ...data }); }); } }] }); };
M.timer = ([id]) => [{ t: 'Edit…', f: () => A.timerEdit([id]) }, { t: 'Delete', f: () => mutate(() => { DOC.timers = DOC.timers.filter(t => t.id !== id); }) }];

TAB.Messages = () => html`${head('Messages', 'messageEdit', html`<button class="btn sm" data-a="messageOnce">Words once…</button>`)}${DOC.messages.map(m => html`<div class="card ${state.message === m.id ? 'shown' : ''}"><div class="t"><b>${m.name}</b><span>${m.text}${m.leaves ? ` · leaves after ${m.leaves}s` : ''} · ${m.look}</span></div>
  <button class="btn" data-a="message:${m.id}">${sw(state.message === m.id, 'Hide', 'Show')}</button>${more('message', m.id)}</div>`)}`;
A.message = ([id]) => { if (state.message === id) { state.message = null; render(); } else showMessage(id); };
A.messageOnce = () => ask('Show words once', 'Words', '', v => { const m = { id: newId('msg'), name: 'Once', text: v, leaves: 15, look: 'Band', once: true }; DOC.messages.push(m); showMessage(m.id); }, { note: 'Leaves by itself after 15 seconds and is not kept' });
A.messageEdit = ([id]) => { const m = id ? byId(DOC.messages, id) : { name: '', text: '', leaves: 0, look: 'Band' };
  openSheet({ title: id ? 'Message' : 'New message', body: html`${field('Name', html`<input class="field" id="mgName" value="${m.name}">`)}${field('Words', html`<input class="field" id="mgText" value="${m.text}">`)}${
      field('Leaves by itself after (seconds, 0 stays)', html`<input class="field" id="mgLeaves" type="number" min="0" value="${m.leaves}">`)}${field('Look', html`<select class="sel" id="mgLook">${options(['Band', 'Box', 'Lower third'], m.look)}</select>`)}`,
    note: `{clock} is the time · {timer:${DOC.timers[0] ? DOC.timers[0].name : 'Name'}} is a timer`,
    buttons: [{ t: 'Cancel' }, { t: 'Save', primary: true, f: () => { const name = $('mgName').value.trim(); if (!name) return false;
      mutate(() => { const data = { name, text: $('mgText').value, leaves: +$('mgLeaves').value || 0, look: $('mgLook').value }; if (id) Object.assign(m, data); else DOC.messages.push({ id: newId('msg'), ...data }); }); } }] }); };
M.message = ([id]) => { const m = byId(DOC.messages, id); return [{ t: 'Edit…', f: () => A.messageEdit([id]) },
  { t: 'Duplicate', f: () => mutate(() => { DOC.messages.push({ ...clone(m), id: newId('msg'), name: m.name + ' copy' }); }) },
  { t: 'Copy', f: () => { state.clip = { type: 'message', m: clone(m) }; toast('Message copied'); } }, { t: 'Paste', dim: !state.clip || state.clip.type !== 'message', f: () => mutate(() => { DOC.messages.push({ ...clone(state.clip.m), id: newId('msg') }); }) },
  { t: 'Delete', f: () => mutate(() => { DOC.messages = DOC.messages.filter(x => x.id !== id); if (state.message === id) state.message = null; }) }]; };

TAB.Props = () => html`${head('Props', 'propNew')}${DOC.props.map(p => html`<div class="card ${state.propsOn.includes(p.id) ? 'shown' : ''}"><div class="lv-pthumb thumb">${slideHTML({ objects: p.objects }, { raw: true })}</div><div class="t"><b>${p.name}</b><span>${p.clears ? `Clears itself after ${p.clears}s` : 'Stays until taken off'}</span></div>
  <button class="btn" data-a="prop:${p.id}">${sw(state.propsOn.includes(p.id), 'Hide', 'Show')}</button>${more('prop', p.id)}</div>`)}`;
A.prop = ([id]) => { const p = byId(DOC.props, id); state.propsOn.includes(id) ? state.propsOn.splice(state.propsOn.indexOf(id), 1) : state.propsOn.push(id); p.left = p.clears || 0; render(); };
A.propNew = () => ask('New prop', 'Name', '', v => { mutate(() => { DOC.props.push({ id: newId('prop'), name: v, clears: 0, objects: [obj('text', 'Text', { text: v, frame: { x: 120, y: 860, w: 900, h: 140 }, style: { size: 60, align: 'left' } })] }); }); editOther('prop', DOC.props[DOC.props.length - 1].id); }, { placeholder: 'Lower third', note: 'It opens in the editor, where a prop is made like a slide' });
M.prop = ([id]) => { const p = byId(DOC.props, id); return [{ t: 'Edit in the Editor', f: () => editOther('prop', id) },
  { t: 'Use a Picture', sub: DOC.media.filter(m => !m.video && !m.deck).map(m => ({ t: m.name, f: () => mutate(() => { p.objects = [obj('image', m.name, { media: m.id, frame: { x: 1420, y: 60, w: 440, h: 248 } })]; }) })) },
  { t: 'Clears Itself', sub: [0, 10, 30, 60].map(t => ({ t: t ? `After ${t} seconds` : 'Never', on: (p.clears || 0) === t, f: () => mutate(() => { p.clears = t; }) })) },
  { t: 'Rename…', f: () => ask('Rename prop', 'Name', p.name, v => mutate(() => { p.name = v; })) },
  { t: 'Delete', f: () => mutate(() => { DOC.props = DOC.props.filter(x => x.id !== id); state.propsOn = state.propsOn.filter(x => x !== id); }) }]; };

const macroChoices = () => [...['all', 'slide', 'media', 'props', 'messages', 'audio', 'announcements'].map(w => ({ do: 'clear', what: w })), ...DOC.messages.map(x => ({ do: 'message', what: 'show', name: x.name })), { do: 'message', what: 'hide' },
  ...DOC.props.flatMap(p => [{ do: 'prop', what: 'show', name: p.name }, { do: 'prop', what: 'hide', name: p.name }]), ...DOC.timers.flatMap(t => ['start', 'stop', 'reset'].map(w => ({ do: 'timer', what: w, name: t.name }))),
  { do: 'audience', on: true }, { do: 'audience', on: false }, ...DOC.songs.map(s => ({ do: 'song', id: s.id })), ...DOC.media.map(x => ({ do: 'media', id: x.id })), ...DOC.songs.map(s => ({ do: 'ann', id: s.id })), ...DOC.versions.flatMap(v => [{ do: 'version', id: v.id }, { do: 'version', id: v.id, keep: true }]), { do: 'version', id: null }];
TAB.Macros = () => html`${head('Macros', 'macroNew')}${DOC.macros.map(m => { const open = state.macroOpen === m.id, steps = m.appSteps ? m.appSteps.map(t => ({ text: t })) : m.steps.map(s => ({ text: stepText(s), gone: stepGone(s) }));
  return html`<div class="lv-macro ${open ? 'open' : ''}" style="--c:${m.color}"><div class="lv-macro-h"><div class="t"><b>${m.name}</b><span>${steps.length} step${steps.length === 1 ? '' : 's'}${m.atStart ? ' · when SoftPresenter opens' : ''}</span></div>
      <button class="btn lv-run" data-a="macro:${m.id}" title="Run this macro now">${I.play} Run</button><button class="iconbtn" data-a="macroOpen:${m.id}" title="Its steps" aria-label="Steps"><span class="sc-chev">${I.chev}</span></button>${more('macro', m.id)}</div>
    ${!open ? '' : html`<div class="lv-steps"><div class="lv-atstart"><span><b>Run when SoftPresenter opens</b><span class="sub">Audience screens off here takes effect before any screen draws, so the audience screens never flash on</span></span>${onoff(m.atStart, `macroAtStart:${m.id}`)}</div>${steps.length ? steps.map((s, n) => m.appSteps ? html`<div class="lv-step"><span class="mono sub">${n + 1}</span><span class="t">${s.text}</span></div>` : html`<div class="lv-step" draggable="true" data-drag="step:${m.id},${n}"><span class="mono sub">${n + 1}</span><button class="t lv-stept ${s.gone ? 'gone' : ''}" data-a="menu:stepPick,${m.id},${n}" data-m="stepPick:${m.id},${n}" title="${s.gone ? 'What this step names has gone: click to choose another' : 'Change what this step does · drag to move it'}"><span>${s.text}</span>${I.chev}</button><button class="iconbtn" data-a="macroStepDel:${m.id},${n}" aria-label="Remove">${I.x}</button></div>`) : html`<p class="ui-p sub">No steps yet</p>`}
      ${m.appSteps ? html`<p class="ui-p sub">Steps are changed in the app</p>` : html`<button class="btn sm lv-stepadd" data-a="menu:stepAdd,${m.id}">${I.plus} Add a step</button>`}</div>`}</div>`; })}`;
A.macro = ([id]) => { const m = byId(DOC.macros, id); m.steps.forEach(runStep); render();  };
A.macroAtStart = ([id]) => mutate(() => { const m = byId(DOC.macros, id); m.atStart = !m.atStart; });      // any number can; they run in list order
A.macroOpen = ([id]) => { state.macroOpen = state.macroOpen === id ? null : id; renderRight(); };
A.macroNew = () => ask('New macro', 'Name', '', v => { mutate(() => { DOC.macros.push({ id: newId('mac'), name: v, color: SWATCHES[4], atStart: false, steps: [] }); state.macroOpen = DOC.macros[DOC.macros.length - 1].id; }); });
const macroSheet = id => { state.tab = 'Macros'; state.macroOpen = id; renderRight(); };
C.macroStepAdd = (v, [id]) => { if (v === '') return; const c = macroChoices()[+v]; mutate(() => { byId(DOC.macros, id).steps.push(c); }); };
// Steps are changed after they are added (the step's own menu, the same choices as Add, in groups) and dragged to reorder
// Title Case, as the app's menus; an empty group stays, with a note, so a volunteer learns what a macro can do
const STEP_GROUPS = [['clear', 'Clear', ''], ['message', 'Messages', 'No messages yet'], ['prop', 'Props', 'No props yet'], ['timer', 'Timers', 'No timers yet'], ['audience', 'Audience Screens', ''], ['song', 'Go to a Song', 'No songs yet'], ['media', 'Show a Picture or Video', 'No pictures or videos yet'], ['ann', 'Announcements', 'No songs yet'], ['version', 'Theme Version', 'No theme has a version yet']];
const sameStep = (a, b) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b);
const stepMenu = (f, now) => STEP_GROUPS.map(([k, name, none]) => { const cs = macroChoices().filter(c => c.do === k); return { t: name, on: !!now && now.do === k, sub: cs.length ? cs.map(c => ({ t: stepText(c), on: sameStep(c, now), f: () => f(c) })) : [{ t: none, dim: true, f: () => {} }] }; });
M.stepPick = ([mid, n]) => { const m = byId(DOC.macros, mid); return [{ head: 'Change this step to' }, ...stepMenu(c => mutate(() => { m.steps[+n] = clone(c); }), m.steps[+n])]; };
M.stepAdd = ([mid]) => [{ head: 'Add a step' }, ...stepMenu(c => mutate(() => { byId(DOC.macros, mid).steps.push(clone(c)); }))];
A.macroStepDel = ([id, n]) => mutate(() => { byId(DOC.macros, id).steps.splice(+n, 1); });
A.macroStepMove = ([id, n, by]) => mutate(() => { const st = byId(DOC.macros, id).steps, x = st.splice(+n, 1)[0]; st.splice(+n + +by, 0, x); });
M.macro = ([id]) => { const m = byId(DOC.macros, id); return [{ t: 'Edit Steps…', f: () => macroSheet(id) }, { t: 'Colour', sub: SWATCHES.slice(2).map(c => ({ t: c, on: m.color === c, f: () => mutate(() => { m.color = c; }) })) },
  { t: 'Run When SoftPresenter Opens', on: m.atStart, f: () => mutate(() => { m.atStart = !m.atStart; }) }, { t: 'Rename…', f: () => ask('Rename macro', 'Name', m.name, v => mutate(() => { m.name = v; })) },
  { t: 'Delete', f: () => mutate(() => { DOC.macros = DOC.macros.filter(x => x.id !== id); }) }]; };

// Stage: what the stage sees, then every layout as a tile to click (one click switches), then the message for the people on stage.
TAB.Stage = () => { const all = stageScreens();
  if (!all.length) return html`<div class="lv-h"><span class="cap">Stage</span></div><p class="ui-p sub">No stage screens yet.</p><button class="btn" data-a="toScreens">Set up screens…</button>`;
  const sel = stageSel(), targets = sel === 'all' ? all : all.filter(s => s.id === sel), shown = targets[0], live = state.stage && shown.on, same = targets.every(s => s.layout === shown.layout);
  return html`${all.length > 1 ? html`<div class="lv-h"><span class="cap">Which stage screen</span><select class="sel sm" data-c="stageSel" aria-label="Which stage screen">${options([['all', 'All stage screens'], ...all.map(s => [s.id, s.name])], sel)}</select></div>` : ''}
    ${live ? '' : html`<div class="lv-stoff">${state.stage ? 'This stage screen is switched off' : 'The stage screens are off'} · the layout still changes, ready for when it is on</div>`}
    <div class="lv-h lv-layh"><span class="cap">${all.length > 1 ? 'Layout' : shown.name + ' · layout'}</span><button class="btn sm" data-a="editLayouts">Edit layouts…</button></div>
    <div class="lv-lays lv-stage">${DOC.stageLayouts.map(lay => { const on = same && shown.layout === lay.id; return html`<button class="lv-lay ${on ? 'on' : ''}" data-a="stageLay:${lay.id}" title="Switch the stage to ${lay.name}"><span class="thumb">${stageHTML(lay.id)}</span><span class="nm">${on ? I.check : ''}${lay.name}</span></button>`; })}</div>
    <span class="cap" style="margin-top:6px">Stage message${state.stageMsg ? ' · showing' : ''}</span><input class="field" id="stageMsg" placeholder="A note for the people on stage" value="${state.stageMsg}">
    <div style="display:flex;gap:6px"><button class="btn ${state.stageMsg ? '' : 'primary'}" data-a="stageMsg:show">${sw(state.stageMsg, 'Change it', 'Show on stage')}</button><button class="btn" data-a="stageMsg:clear" ${{ disabled: !state.stageMsg }}>Clear</button></div>`; };
// which stage screens the layout tiles act on: the one chosen, or all of them (the usual case) when nothing is chosen
const stageSel = () => { const all = stageScreens(); return state.stageSel && all.some(s => s.id === state.stageSel) ? state.stageSel : all.length > 1 ? 'all' : (all[0] || {}).id; };
A.stageLay = ([id]) => mutate(() => { const all = stageScreens(), sel = stageSel(); (sel === 'all' ? all : all.filter(s => s.id === sel)).forEach(s => { s.layout = id; });  });
C.stageSel = v => { state.stageSel = v === 'all' ? null : v; render(); };
C.stageLayout = (v, [id]) => mutate(() => { byId(DOC.screens, id).layout = v; });
A.stageMsg = ([what]) => { state.stageMsg = what === 'show' ? $('stageMsg').value : ''; render(); toast(state.stageMsg ? 'The stage sees the message' : 'Stage message cleared'); };
A.editLayouts = () => editOther('layout', DOC.stageLayouts[0].id);

const audioPl = () => byId(DOC.audio, state.audio.pl) || DOC.audio[0];
// Audio: the player first (what is playing, where in it, the keys, the level), then the list it plays from.
TAB.Audio = () => { const a = state.audio, pl = audioPl(), tr = pl && pl.tracks[a.track], total = pl && pl.tracks.every(t => t.length) ? pl.tracks.reduce((n, t) => n + t.length, 0) : 0;
  const player = html`<div class="lv-audio ${a.playing ? 'playing' : ''}">
      <div class="lv-now"><span class="cap">${a.playing ? 'Playing' : tr ? 'Ready' : 'Nothing to play'}</span><b class="${tr ? 'lv-tname' : ''}">${tr ? tr.name : 'Add a track to this playlist'}</b>${tr && !tr.url && !(typeof REMOTE === 'object' && REMOTE.on) ? html`<span class="sub">A sample track: it runs, but makes no sound</span>` : ''}</div>
      <div class="lv-seek"><span class="tm mono" id="aGone">${mmss(a.pos)}</span><input type="range" id="aSeek" min="0" max="${tr ? tr.length : 0}" value="${a.pos}" data-i="seek" aria-label="Where in the track" ${{ disabled: !tr }} style="--p:${tr && tr.length ? a.pos / tr.length * 100 : 0}%"><span class="tm mono" id="aLeft">${tr ? '−' + mmss(Math.max(0, tr.length - a.pos)) : '0:00'}</span></div>
      <div class="lv-keys"><button class="vk" data-a="audio:prev" title="Previous track">${I.prev}</button><button class="vk wide" data-a="audio:back" title="Back 15 seconds">−15</button><button class="vk big" data-a="audio:play" title="${a.playing ? 'Pause' : 'Play'}" aria-label="${a.playing ? 'Pause' : 'Play'}">${a.playing ? I.pause : I.play}</button>
        <button class="vk wide" data-a="audio:fwd" title="Forward 15 seconds">+15</button><button class="vk" data-a="audio:next" title="Next track">${I.next}</button></div>
      <div class="lv-vol"><button class="iconbtn ${a.muted ? 'lit' : ''}" data-a="audio:mute" title="${a.muted ? 'Sound back on' : 'Mute'}" aria-label="Mute">${a.muted ? I.mute : I.sound}</button><input type="range" min="0" max="100" value="${a.volume}" data-i="volume" aria-label="Volume" style="--p:${a.volume}%"><span class="tm mono" id="aVol">${a.muted ? 'Muted' : a.volume + '%'}</span></div>
      <div class="lv-opts"><button class="btn sm ${a.through ? 'onkey' : ''}" data-a="audio:through" title="When a track ends, go on to the next">Play through</button><button class="btn sm ${a.shuffle ? 'onkey' : ''}" data-a="audio:shuffle" title="Play the tracks in a random order">Shuffle</button><button class="btn sm ${a.loop ? 'onkey' : ''}" data-a="audio:loop" title="Keep playing until stopped: after the last track, start again from the first (with Play through off, play this track again)">Loop</button></div>
      <div class="lv-opts"><span class="cap">Crossfade</span><select class="sel sm" data-c="crossfade" aria-label="Crossfade" title="How long one track fades into the next">${options([0, 1, 3, 5, 10].map(n => [n, n ? `${n} s` : 'Off']), a.crossfade)}</select></div></div>`;
  const list = pl ? html`<div class="lv-h lv-listh"><span class="cap">${pl.tracks.length} track${pl.tracks.length === 1 ? '' : 's'}${total ? html` · ${mmss(total)}` : ''}</span><button class="btn sm" data-a="audio:add">${I.plus} Add tracks</button></div>${
      pl.tracks.length ? pl.tracks.map((t, n) => html`<div class="lv-track ${n === a.track ? 'cur' : ''} ${n === a.track && a.playing ? 'on' : ''}" draggable="true" data-drag="track:${pl.id},${n}" data-m="track:${n}"><button class="lv-tplay" data-a="audio:track,${n}" title="Play this track"><span class="mono n">${n === a.track && a.playing ? I.play : two(n + 1)}</span><span class="t">${t.name}</span><span class="mono len">${t.length ? mmss(t.length) : ''}</span></button>${more('track', n)}</div>`)
        : html`<div class="sh-none">No tracks yet. Add music files from this computer.</div>`}` : '';
  return html`${head('Audio', 'audioNew', html`<select class="sel sm" data-c="audioPl" aria-label="Audio playlist">${options(DOC.audio.map(p => [p.id, p.name]), pl && pl.id)}</select>${pl ? more('audioPl', pl.id) : ''}`)}${player}${list}`; };
// A track has ended (the app, 9bc1cc6). Play through goes on to the next and stops after the last; with Loop it starts
// again from the first (shuffled: a fresh order each pass, never the same track twice across the wrap). Loop with Play
// through off plays this track again.
function audioEnded(a, pl) { a.pos = 0; const n = pl.tracks.length;
  if (!a.through) { if (!a.loop) a.playing = false; return; }
  a.played = (a.played || []).concat(a.track); let left = [...Array(n).keys()].filter(k => !a.played.includes(k));
  if (!left.length) { a.played = []; if (!a.loop) { a.playing = false; return; } left = [...Array(n).keys()].filter(k => n === 1 || k !== a.track); }
  a.track = a.shuffle ? left[Math.floor(Math.random() * left.length)] : left.includes(a.track + 1) ? a.track + 1 : left[0]; }
A.audio = ([what, n]) => { const a = state.audio, pl = audioPl(); if (!pl) return;
  const to = k => { a.track = (k + pl.tracks.length) % pl.tracks.length; a.pos = 0; };
  if (what === 'track') { to(+n); a.playing = true; } if (what === 'play') a.playing = !a.playing && pl.tracks.length > 0;
  if (what === 'next') to(a.track + 1); if (what === 'prev') { if (a.pos > 3) a.pos = 0; else to(a.track - 1); }      // Previous first goes back to the start of the track, as players do
  if (what === 'fwd' || what === 'back') a.pos = Math.max(0, Math.min((pl.tracks[a.track] || { length: 0 }).length, a.pos + (what === 'fwd' ? 15 : -15)));
  if (what === 'through' || what === 'shuffle' || what === 'loop' || what === 'mute') a[what === 'mute' ? 'muted' : what] = !a[what === 'mute' ? 'muted' : what];
  if (what === 'add') return pickFiles('audio/*', addAudioFiles);
  syncAudio(); render(); };
// a track's own menu, and the playlist's
function trackMove(pl, from, to) { const a = state.audio, cur = pl.tracks[a.track], t = pl.tracks.splice(from, 1)[0]; pl.tracks.splice(to, 0, t); a.track = Math.max(0, pl.tracks.indexOf(cur)); }
M.track = ([n]) => { const pl = audioPl(), a = state.audio; n = +n; return [
  { t: 'Play', f: () => A.audio(['track', n]) }, { sep: true },
  { t: 'Move Up', dim: n === 0, f: () => mutate(() => trackMove(pl, n, n - 1)) }, { t: 'Move Down', dim: n === pl.tracks.length - 1, f: () => mutate(() => trackMove(pl, n, n + 1)) },
  { t: 'Rename…', f: () => ask('Rename track', 'Name', pl.tracks[n].name, v => mutate(() => { pl.tracks[n].name = v; })) },
  { t: 'Remove', f: () => mutate(() => { const cur = pl.tracks[a.track]; pl.tracks.splice(n, 1); if (n === a.track) { a.playing = false; a.pos = 0; a.track = Math.min(n, Math.max(0, pl.tracks.length - 1)); } else a.track = pl.tracks.indexOf(cur); syncAudio(); }) }]; };
M.audioPl = ([id]) => { const pl = byId(DOC.audio, id); return [
  { t: 'Rename…', f: () => ask('Rename audio playlist', 'Name', pl.name, v => mutate(() => { pl.name = v; })) },
  { t: 'New Audio Playlist…', f: () => A.audioNew() },
  { t: 'Delete…', dim: DOC.audio.length === 1, f: () => undoable(`Delete “${pl.name}”?`, 'Its tracks leave the list; the files stay on the computer.', 'Delete', () => mutate(() => { DOC.audio.splice(DOC.audio.indexOf(pl), 1); Object.assign(state.audio, { pl: DOC.audio[0].id, track: 0, pos: 0, playing: false }); syncAudio(); })) }]; };
A.audioNew = () => { const p = { id: newId('au'), name: 'New Audio Playlist', tracks: [] }; mutate(() => { DOC.audio.push(p); state.audio.pl = p.id; state.audio.track = 0; }); ask('Name the audio playlist', 'Name', p.name, v => mutate(() => { p.name = v; })); };      // made with its default name, the name ready to type
C.audioPl = v => { Object.assign(state.audio, { pl: v, track: 0, pos: 0, playing: false }); render(); };
C.crossfade = v => { state.audio.crossfade = +v; };

TAB.VoxLive = () => html`<div class="lv-h"><span class="cap">VoxLive screens</span></div>${DOC.voxlive.screens.map(v => html`<div class="card ${state.vox === v.id ? 'shown' : ''}"><div class="t"><b>${v.name}</b><span>${v.lang} · ${v.mode}</span></div>
  <button class="btn" data-a="vox:${v.id}">${sw(state.vox === v.id, 'On screen', 'Show')}</button></div>`)}<p class="ui-p sub">Live captions take the place of the slide. Click a verse or a slide to bring it back.</p>`;
A.vox = ([id]) => { if (state.vox === id) return clear('slide'); const n = DOC.voxlive.screens.findIndex(v => v.id === id); go('vox', n); };

/* ═════════ bible.js ═════════ */
// Bible: look a passage up, read the chapter, tap a verse to show it. Up to four translations on a slide.
'use strict';

VIEWS.bible = { left: renderBibleLeft, centre: renderBibleCentre };
const B = () => state.bible;
const hasText = (book, ch) => !!(BIBLE_DATA.text.kjv[book] && BIBLE_DATA.text.kjv[book][ch]);
const bookByName = n => BOOKS.find(b => b.name === n);
// What language the operator reads the Bible page in: book names, the reference field, the page's verse text, the find list's
// Bible rows. Auto: the language typed, else the last one typed, else the computer's; only languages with a Bible in use.
// It never changes the slides on screen; those follow the translations line.
const LANG_NAMES = { en: 'English', ru: 'Русский', uk: 'Українська' }, LANG_CODES = { en: 'EN', ru: 'RU', uk: 'UK' };
const bibleLang = id => (BIBLE_DATA.bibles[id] || {}).lang;
const langsInUse = () => [...new Set(B().rows.filter(x => BIBLE_DATA.text[x.bible]).map(x => bibleLang(x.bible)))];
const bibleIn = lang => (B().rows.find(x => BIBLE_DATA.text[x.bible] && bibleLang(x.bible) === lang) || {}).bible;
const langOfText = t => /[іїєґ]/i.test(t) ? 'uk' : /[а-яё]/i.test(t) ? 'ru' : /[a-z]/i.test(t) ? 'en' : null;
function noteTyped(t) { const l = langOfText(t || ''); if (l) B().typedLang = l; }
function readLang(hint) {
  const use = langsInUse(), set = DOC.settings.bibleLang || 'auto';
  if (set !== 'auto' && use.includes(set)) return set;
  return [langOfText(hint || ''), B().typedLang, (navigator.language || 'en').slice(0, 2)].find(l => l && use.includes(l)) || use[0] || 'en';
}
const bookNameIn = (name, lang) => lang === 'ru' ? BOOK_NAMES_RU[name] || name : name;
const chapterTitle = (r, lang) => `${bookNameIn(r.book, lang)} ${lang === 'ru' && r.book === 'Psalm' ? r.chapter - 1 : r.chapter}`;
M.bibleLang = () => { const use = langsInUse(), set = DOC.settings.bibleLang || 'auto';
  return [{ head: 'Read the Bible page in' }, { t: `Auto · ${LANG_NAMES[readLang()]}`, on: set === 'auto', f: () => A.bibleLangSet(['auto']) },
    ...['en', 'ru', 'uk'].map(l => ({ t: LANG_NAMES[l] + (use.includes(l) ? '' : ' · no Bible in use'), on: set === l, dim: !use.includes(l), f: () => use.includes(l) && A.bibleLangSet([l]) })), { sep: true }, { t: 'The slides on screen keep the translations line', dim: true, f: () => {} }]; };
A.bibleLangSet = ([l]) => { DOC.settings.bibleLang = l; render(); };
A.bibleBooks = ([v]) => { DOC.settings.bibleBooks = v; renderBibleLeft(); };
const allBibles = () => [...Object.entries(BIBLE_DATA.bibles).map(([id, b]) => ({ id, ...b, text: true })), ...B().installed.map(n => ({ id: 'dl-' + n, name: n, short: n.split(' ').map(w => w[0]).join('').slice(0, 4).toUpperCase(), text: false }))];

// "jn 3 16", "John 3:16-18", "Psalm 23", "Пс 22:1-3", "Ів 3:16"
function parseRef(text) {
  const m = /^\s*((?:[1-3]\s*)?\p{L}{2,})\.?\s*(\d+)?(?:\s*[:.,\s]\s*(\d+)(?:\s*[-–]\s*(\d+))?)?\s*$/u.exec(text || '');
  if (!m) return null;
  const typed = m[1].toLowerCase().replace(/\s+/g, ' ');
  let name = BOOK_ALIASES[typed], shift = 0;
  if (name && name.endsWith('*')) { name = name.slice(0, -1); shift = 1; }       // a Russian Psalm number is one less
  if (!name) { const b = BOOKS.find(x => x.name.toLowerCase().startsWith(typed)) || BOOKS.find(x => x.code.toLowerCase() === typed); if (!b) return null; name = b.name; }
  const book = bookByName(name), chapter = m[2] ? +m[2] + shift : (Object.keys(BIBLE_DATA.text.kjv[name] || {})[0] | 0) || 1;
  if (!hasText(name, chapter)) return { missing: true, book: name, chapter };
  const vs = chapterVerses(name, chapter), from = m[3] ? +m[3] : vs[0], to = m[4] ? +m[4] : (m[3] ? from : vs[vs.length - 1]);
  if (!vs.includes(from)) return { missing: true, book: name, chapter };
  return { book: name, chapter, from, to: Math.min(to, vs[vs.length - 1]), code: book.code };
}
const bookHints = text => { const m = /^\s*((?:[1-3]\s*)?\p{L}+)\s*$/u.exec(text || ''); if (!m) return []; const t = m[1].toLowerCase(); return BOOKS.filter(b => b.name.toLowerCase().startsWith(t)).slice(0, 6); };

function lookUp(ref, quiet) {
  const b = B(), same = b.ref.book === ref.book && b.ref.chapter === ref.chapter;
  // the verse on screen stays there, and Space keeps reading on through its chapter, while the new one is looked at
  if (!same && state.live && state.live.deck === 'bible') { state.heldBible = { ...bibleDeck(), key: 'bible-held', ref: { ...b.ref } }; state.live = { ...state.live, deck: 'bible-held' }; }
  b.ref = { book: ref.book, chapter: ref.chapter, from: ref.from, to: ref.to };
  if (!quiet) {      // the passage's first verse is cued, nothing goes on screen, and the keys come back to the window
    const d = bibleDeck(), at = b.opts.per === 'verse' ? Math.max(0, d.slides.findIndex(sl => sl.v === ref.from)) : 0;
    state.mode = 'bible'; setCue('bible', at); if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
    render(); const el = document.querySelector('.cued') || document.querySelector('.inrange'); if (el) el.scrollIntoView({ block: 'center' });
  }
}
function noteHistory() {
  const d = bibleDeck(), sl = d.slides[state.live.s], r = B().ref, per = B().opts.per === 'verse';
  const ref = per ? { ...r, from: sl.v, to: sl.v } : { ...r }, title = refTitle(ref);
  B().history = [{ ref, title, at: Date.now() }, ...B().history.filter(h => h.title !== title)].slice(0, 100);
}

/* ───────── left: books, chapters and verses as keys; or the history ───────── */
function renderBibleLeft() {
  const b = B(), r = b.ref, book = bookByName(r.book);
  const tabs = html`<div class="bi-side">${seg([['books', 'Books'], ['history', 'History']], b.side, 'bibleSide')}<button class="btn sm" data-a="bibleLists" title="Put the lists away">Hide</button></div>`;
  if (b.side === 'history') {
    put('left', tabs, html`<div class="sect">${b.history.length ? b.history.map((h, n) => html`<button class="rowbtn" data-a="bibleHist:${n}"><span class="t">${h.title}</span><span class="meta mono">${new Date(h.at).toLocaleTimeString('en-GB').slice(0, 5)}</span></button>`) : html`<div class="sh-none">Passages that were on screen are listed here, newest first.</div>`}
      ${b.history.length ? html`<button class="btn sh-add" data-a="bibleHistClear" style="margin-top:10px">Clear history</button>` : ''}</div>`);
    return;
  }
  const lang = readLang(), ru = lang === 'ru', lists = DOC.settings.bibleBooks === 'lists';
  const pick = html`<span class="sp"></span>${seg([['keypad', I.grid], ['lists', I.list]], lists ? 'lists' : 'keypad', 'bibleBooks')}`;
  if (lists) {       // the app's scrolling lists, restyled: book, chapter and verse side by side, each scrolling on its own
    const row = (cls, a, text, on, has = true, title = '') => html`<button class="bl-row ${cls} ${on ? 'on' : ''} ${has ? 'has' : ''}" data-a="${a}" ${{ title: title || null }}>${String(text)}</button>`;
    const bl = BOOKS.map((x, n) => [n === 0 ? html`<span class="bl-h">Old Testament</span>` : n === 39 ? html`<span class="bl-h">New Testament</span>` : '', row('', `bibleBook:${x.code}`, ru ? RU_FULL[n] : x.name, x.name === r.book, !!BIBLE_DATA.text.kjv[x.name])]);
    const cl = Array.from({ length: book.chapters }, (_, n) => n + 1).map(c => row('num', `bibleChapter:${c}`, ru && r.book === 'Psalm' ? c - 1 || c : c, c === r.chapter, hasText(r.book, c)));
    const vl = chapterVerses(r.book, r.chapter).map(v => row('num', `bibleVerse:${v}`, v, v >= r.from && v <= r.to, true, 'Shift-click another to take the passage between them'));
    put('left', tabs, html`<div class="sect-h bl-head"><span class="bl-hbook"><span class="cap">Book</span>${pick}</span><span class="cap">Ch</span><span class="cap">Verse</span></div><div class="bi-lists"><div class="bl-col bl-books">${bl}</div><div class="bl-col">${cl}</div><div class="bl-col">${vl}</div></div>`);
    for (const col of document.querySelectorAll('.bl-col')) { const on = col.querySelector('.on'); if (on) col.scrollTop = on.offsetTop - col.clientHeight / 2 + on.offsetHeight / 2; }      // each list shows its chosen row
    return;
  }
  const books = BOOKS.map((x, n) => html`${n === 39 ? html`<i class="gap"></i>` : ''}<button class="bk ${BIBLE_DATA.text.kjv[x.name] ? 'has' : ''} ${x.name === r.book ? 'on' : ''}" data-a="bibleBook:${x.code}" title="${ru ? RU_FULL[n] : x.name}">${ru ? RU_SHORT[n] : x.code[0] + x.code.slice(1).toLowerCase()}</button>`);
  const chapters = Array.from({ length: book.chapters }, (_, n) => n + 1).map(c => html`<button class="bk ${hasText(r.book, c) ? 'has' : ''} ${c === r.chapter ? 'on' : ''}" data-a="bibleChapter:${c}">${ru && r.book === 'Psalm' ? c - 1 || c : c}</button>`);      // numbered in the language read, like the title and the lists, so the lit key and the title agree
  const verses = chapterVerses(r.book, r.chapter).map(v => html`<button class="bk has ${v >= r.from && v <= r.to ? 'on' : ''}" data-a="bibleVerse:${v}">${v}</button>`);
  put('left', tabs, html`<div class="sect"><div class="sect-h"><span class="cap">Book</span>${pick}</div><div class="bookgrid">${books}</div></div>
    <div class="sect"><div class="sect-h"><span class="cap">${bookNameIn(r.book, lang)} · chapter</span></div><div class="chgrid">${chapters}</div></div>
    <div class="sect" style="padding-bottom:14px"><div class="sect-h"><span class="cap" title="Click a verse; Shift-click another to take the passage between them">Verse <span class="bi-tip">· ⇧ for a passage</span></span></div><div class="chgrid">${verses}</div></div>`);
}
A.bibleSide = ([s]) => { B().side = s; renderBibleLeft(); };
A.bibleLists = () => { B().lists = !B().lists; render(); };
A.bibleBook = ([code]) => { const b = BOOKS.find(x => x.code === code), r = parseRef(b.name); return r && !r.missing ? lookUp(r) : refuse(`This sample only has ${BIBLE_SAMPLE}`); };
A.bibleChapter = ([c]) => { const r = parseRef(`${B().ref.book} ${c}`); return r && !r.missing ? lookUp(r) : refuse(`This sample only has ${BIBLE_SAMPLE}`); };
A.bibleVerse = ([v], el, ev) => { const r = B().ref; v = +v; lookUp(ev && ev.shiftKey ? { ...r, from: Math.min(r.from, v), to: Math.max(r.from, v) } : { ...r, from: v, to: v }); };
A.bibleHist = ([n]) => lookUp(B().history[+n].ref);
A.bibleHistClear = () => { B().history = []; renderBibleLeft(); };

/* ───────── middle ───────── */
function renderBibleCentre() {
  const b = B(), r = b.ref, rows = b.rows;
  $('media').hidden = false; $('foot').hidden = true;
  $('slides').className = 'reader';
  const short = id => id === 'none' ? 'None' : (allBibles().find(x => x.id === id) || { short: '?' }).short;
  const table = b.fold ? html`<div class="bi-tr"><span class="cap">Translations</span>${rows.map(x => html`<span class="pill" title="Verse in ${x.verse}, reference in ${x.ref}">${short(x.bible)}</span>`)}<button class="btn sm" data-a="bibleFold">Change</button></div>`
    : html`<div class="bi-rows">${rows.map((x, n) => html`<div class="bi-row"><select class="sel" data-c="bibleRow:${n},bible" aria-label="Bible">${options([...allBibles().map(q => [q.id, q.name + (q.text ? '' : ' (no sample text)')]), ['none', 'None']], x.bible)}</select>
        <span class="cap">Verse in</span><select class="sel" data-c="bibleRow:${n},verse">${options([['verse1', 'Verse 1 box'], ['verse2', 'Verse 2 box']], x.verse)}</select>
        <span class="cap">Reference in</span><select class="sel" data-c="bibleRow:${n},ref">${options([['ref1', 'Reference 1 box'], ['ref2', 'Reference 2 box'], ['none', 'Nowhere']], x.ref)}</select>
        <button class="iconbtn" data-a="bibleRowDel:${n}" title="Take this translation off" ${{ disabled: rows.length === 1 }}>${I.x}</button></div>`)}
      <div class="bi-rowf">${rows.length < 4 ? html`<button class="btn sm" data-a="bibleRowAdd">${I.plus} Another translation</button>` : html`<span class="sub">Four is the most</span>`}<button class="btn sm" data-a="biblesSheet">Manage Bibles…</button><span class="sp"></span><button class="btn sm primary" data-a="bibleFold">Done</button></div></div>`;
  const pl = state.src.type === 'pl' ? playlist() : DOC.playlists[0];
  put('slides', html`<div class="reader-h bi-head"><div class="bi-look"><input class="field" id="bRef" data-i="bref" placeholder="John 3:16-18 · Пс 22 · or words to search" value="${refTitle(r, bibleIn(readLang()))}" autocomplete="off" spellcheck="false"><div class="bi-hint" id="bHint" hidden></div></div>
      ${b.lists ? '' : html`<button class="btn" data-a="bibleLists" title="Bring the book, chapter and verse lists back">Books</button>`}<button class="btn sq" data-a="bibleGrow:-1" title="A verse longer at the start">‹</button><button class="btn sq" data-a="bibleGrow:1" title="A verse longer at the end">›</button>
      <span class="sp"></span><span class="bi-add"><button class="btn" data-a="bibleAdd:${pl.id}" title="Add ${refTitle(r)} to “${pl.name}”">${I.plus} Add</button><button class="btn sq" data-a="menu:bibleAdd" title="Add it to another playlist" aria-label="Another playlist">▾</button></span><i class="bi-div"></i>${seg([['page', 'Page'], ['slides', 'Slides']], b.view, 'bibleView')}<button class="btn bi-lang" data-a="menu:bibleLang" title="The language you read this page in: book names, the reference and the verse text. The slides on screen don’t change."><b>Аа</b>${DOC.settings.bibleLang && DOC.settings.bibleLang !== 'auto' ? LANG_CODES[readLang()] : 'Auto'}</button><button class="btn" data-a="bibleOptions">Options</button>
</div>
    ${table}<div id="bBody"></div>`);
  renderBibleBody();
}
function renderBibleBody(search) { drawBibleBody(search); const b = $('bBody'); if (b) fitAll(b); }
function drawBibleBody(search) {
  const b = B(), r = b.ref, d = bibleDeck(), lang = readLang(), avail = b.rows.filter(x => x.bible !== 'none' && BIBLE_DATA.text[x.bible]).sort((x, y) => (bibleLang(y.bible) === lang) - (bibleLang(x.bible) === lang)), first = avail[0], second = avail[1];      // the language read first
  const body = $('bBody'); if (!body) return;
  if (search) {
    const want = search.toLowerCase(), hits = [];
    const seen = new Set();       // every translation on the slide is searched
    for (const row of b.rows.filter(x => BIBLE_DATA.text[x.bible])) { const src = BIBLE_DATA.text[row.bible];
      for (const book in src) for (const ch in src[book]) for (const v in src[book][ch]) { const key = `${book} ${ch}:${v}`; if (hits.length < 30 && !seen.has(key) && src[book][ch][v].toLowerCase().includes(want)) { seen.add(key); hits.push({ book, ch: +ch, v: +v, text: src[book][ch][v] }); } } }
    put(body, html`<h1 class="bi-title">“${search}”<span class="pill">${hits.length} verse${hits.length === 1 ? '' : 's'}</span></h1>`, hits.length ? hits.map(h => html`<div class="bi-hit"><button class="verse" data-a="bibleOpen:${h.book},${h.ch},${h.v},${h.v}"><span class="v">${h.book} ${h.ch}:${h.v}</span><span class="t">${h.text}</span></button><button class="btn sm" data-a="bibleOpen:${h.book},${h.ch},1,999">Whole chapter</button></div>`) : html`<div class="note">No verse in the sample has those words.</div>`);
    return;
  }
  if (!first) { put(body, html`<div class="note">Choose a Bible that has text in this sample.</div>`); return; }
  const per = b.opts.per === 'verse', vs = chapterVerses(r.book, r.chapter);
  const pl = state.src.type === 'pl' ? playlist() : DOC.playlists[0];
  const title = html`<h1 class="bi-title">${per ? chapterTitle(r, lang) : refTitle(r, first.bible)}<span class="pill">${BIBLE_DATA.bibles[first.bible].short}${second ? ' + ' + BIBLE_DATA.bibles[second.bible].short : ''}</span></h1>`;
  const note = html`<div class="note">Click a verse to put it on screen; Space and the arrows move on through the chapter. This sample holds ${BIBLE_SAMPLE}.</div>`;
  requestAnimationFrame(() => { const h = document.querySelector('.bi-head'); if (h) body.style.setProperty('--bi-top', h.offsetHeight + 'px'); });      // the title stays put under the top bar
  if (b.view === 'slides') { put(body, title, html`<div class="grid">${d.slides.map((s, k) => html`<button class="slide ${isLive('bible', k) ? 'live' : ''} ${isCued('bible', k, k) ? 'cued' : ''} ${per && s.v >= r.from && s.v <= r.to ? 'inrange' : ''}" style="--g:var(--c-bible)" data-a="go:bible,${k}">${thumb(s, d.ctx)}<div class="strip"><span class="n">${two(k + 1)}</span><i class="dot"></i><span class="lbl">${s.group}</span></div></button>`)}</div>`, note); return; }
  // where the other Bible numbers this chapter differently (the Psalms, Russian against English), its line starts with its own reference
  const oNum = second && r.book === 'Psalm' && (bibleLang(second.bible) === 'ru') !== (bibleLang(first.bible) === 'ru');
  put(body, title, vs.map((v, k) => { const k2 = per ? k : 0, inr = v >= r.from && v <= r.to;
    return html`<button class="verse ${isLive('bible', k2) && (per || inr) ? 'live' : ''} ${(per ? isCued('bible', k, k) : inr && isCued('bible', 0, 0)) ? 'cued' : ''} ${inr ? 'inrange' : ''}" data-a="${per ? `go:bible,${k}` : inr ? 'go:bible,0' : `bibleVerse:${v}`}"><span class="v">${v}</span><span class="t">${bibleText(first.bible, r.book, r.chapter, v)}${second ? html`<i>${oNum ? html`<span class="v-oref">${refTitle({ book: r.book, chapter: r.chapter, from: v, to: v }, second.bible)}</span>` : ''}${bibleText(second.bible, r.book, r.chapter, v)}</i>` : ''}</span></button>`; }), note);
}
A.bibleOpen = ([book, ch, from, to]) => { const vs = chapterVerses(book, +ch); lookUp({ book, chapter: +ch, from: +from, to: Math.min(+to, vs[vs.length - 1]) }); };
A.bibleFold = () => { B().fold = !B().fold; render(); };
A.bibleView = ([v]) => { B().view = v; render(); };
A.bibleGrow = ([d]) => { const r = B().ref, vs = chapterVerses(r.book, r.chapter); if (+d < 0 && r.from > vs[0]) r.from--; if (+d > 0 && r.to < vs[vs.length - 1]) r.to++; render(); };
C.bibleRow = (v, [n, prop]) => { B().rows[+n][prop] = v; render(); };
A.bibleRowAdd = () => { const used = B().rows.map(r => r.bible), next = ['web', 'bsb', 'kjv', 'syn'].find(x => !used.includes(x)) || 'none'; B().rows.push({ bible: next, verse: 'verse2', ref: 'none' }); render(); };
A.bibleRowDel = ([n]) => { B().rows.splice(+n, 1); render(); };
const samePassage = (a, b) => a.book === b.book && a.chapter === b.chapter && a.from === b.from && a.to === b.to;
A.bibleAdd = ([plId]) => { const p = byId(DOC.playlists, plId), r = { ...B().ref };
  if (p.items.some(it => it.kind === 'passage' && samePassage(it.passage, r))) return refuse(`${refTitle(r)} is already in ${p.name}`);
  mutate(() => { p.items.push({ id: newId('it'), kind: 'passage', passage: r }); }); toast(`${refTitle(r)} added to ${p.name}`); };      // out of sight from the Bible page, so it says so
M.bibleAdd = () => { const front = state.src.type === 'pl' ? playlist() : DOC.playlists[0];
  return [...DOC.playlists.filter(p => p !== front).map(p => ({ t: `Add to “${p.name}”`, f: () => A.bibleAdd([p.id]) })), { sep: true },
    { t: 'New Playlist…', f: () => { const p = { id: newId('pl'), name: 'New Playlist', items: [{ id: newId('it'), kind: 'passage', passage: { ...B().ref } }] }; mutate(() => { DOC.playlists.push(p); state.src = { type: 'pl', id: p.id }; state.mode = 'show'; });
      renameAt('.plbtn > span', p.name, v => mutate(() => { p.name = v; }), 'Name the playlist'); } }]; };      // in front, in the Service view, its name ready to type

// The field follows what is typed a moment after typing stops; nothing goes on screen until a verse is clicked.
let brefTimer;
function typedRef(text) {
  clearTimeout(brefTimer); noteTyped(text);
  const hint = $('bHint'), hints = bookHints(text), ref = parseRef(text);
  B().hint = hints;
  hint.hidden = !hints.length || (hints.length === 1 && hints[0].name.toLowerCase() === text.trim().toLowerCase());
  put(hint, hints.map((h, n) => html`<button data-a="bibleHint:${n}">${h.name}${BIBLE_DATA.text.kjv[h.name] ? '' : html` <i>not in this sample</i>`}</button>`));
  brefTimer = setTimeout(() => {
    if (ref && !ref.missing) { lookUp(ref, true); renderBibleLeft(); renderBibleBody(); }
    else if (ref && ref.missing) { const b = $('bBody'); if (b) put(b, html`<div class="note">This sample only has ${BIBLE_SAMPLE}.</div>`); }
    else if (text.trim().length >= 3 && !hints.length) renderBibleBody(text.trim());
  }, 250);
}
A.bibleHint = ([n]) => { const h = B().hint[+n], f = $('bRef'); f.value = h.name + ' '; f.focus(); typedRef(f.value); };

/* ───────── options, with a preview of the first and last slides ───────── */
A.bibleOptions = () => {
  const o = B().opts, r = B().ref, sl = passageSlides(r), ctx = { kind: 'bible', theme: o.theme };
  openSheet({ title: 'Bible slides', wide: true, body: html`<div class="bi-opts"><div>
      ${field('Each slide shows', html`<select class="sel" data-c="bibleOpt:per">${options([['verse', 'One verse'], ['passage', 'The whole passage']], o.per)}</select>`)}
      ${field('The reference goes', html`<select class="sel" data-c="bibleOpt:refAt">${options([['every', 'On every slide'], ['passage-every', 'The whole passage, on every slide'], ['last', 'The whole passage, on the last slide'], ['none', 'Nowhere']], o.refAt)}</select>`)}
      ${field('Theme', html`<select class="sel" data-c="bibleOpt:theme">${options([['', 'The main theme'], ...DOC.themes.map(t => [t.id, t.name])], o.theme || '')}</select>`)}
      <label class="ui-check"><input type="checkbox" data-c="bibleOpt:numbers" ${{ checked: !!o.numbers }}> Verse numbers</label>
      <label class="ui-check"><input type="checkbox" data-c="bibleOpt:name" ${{ checked: !!o.name }}> The translation's name</label>
      <button class="btn" data-a="biblesSheet" style="margin-top:12px">Manage Bibles…</button></div>
    <div class="bi-prev"><span class="cap">First slide</span>${thumb(sl[0], ctx)}<span class="cap">Last slide</span>${thumb(sl[sl.length - 1], ctx)}</div></div>`, buttons: [{ t: 'Done', primary: true }] });
};
C.bibleOpt = (v, [k], el) => { B().opts[k] = el.type === 'checkbox' ? el.checked : (v || (k === 'theme' ? null : v)); if (state.live && state.live.deck === 'bible') { state.live = null; state.on = null; } render(); A.bibleOptions(); };

/* ───────── the Bibles on this computer, and those that can be downloaded ───────── */
A.biblesSheet = () => {
  const inst = B().installed;
  openSheet({ title: 'Bibles', wide: true, body: html`<input class="field" placeholder="Search" data-i="biblesFilter" style="margin-bottom:12px"><div id="biblesList">${biblesList('')}</div>`, note: 'Each is public domain or free to share; its terms are shown before it is installed. A Bible of your own installs from a file: Zefania, OSIS, Beblia, USFM or USX, zipped too.', buttons: [{ t: 'Install from File…', f: () => { A.bibleFromFile(); return false; } }, { t: 'Done', primary: true }] });
  state.sheet.inst = inst;
};
function biblesList(f) {
  const hit = n => !f || n.toLowerCase().includes(f.toLowerCase()), inst = B().installed;
  return html`<span class="cap">Installed</span>${Object.values(BIBLE_DATA.bibles).filter(b => hit(b.name)).map(b => html`<div class="ui-row"><span class="t">${b.name}</span><span class="pill">Built in</span></div>`)}
    ${inst.filter(hit).map(n => html`<div class="ui-row"><span class="t">${n}</span><button class="btn sm" data-a="bibleRemove:${encodeURIComponent(n)}">Remove</button></div>`)}
    ${BIBLES_TO_DOWNLOAD.map(([lang, list]) => { const rows = list.filter(n => hit(n) && !inst.includes(n)); return rows.length ? [html`<span class="cap" style="margin-top:12px;display:block">${lang}</span>`, rows.map(n => html`<div class="ui-row"><span class="t">${n}</span><span class="sub">Public domain</span><button class="btn sm" data-a="bibleInstall:${encodeURIComponent(n)}">Install</button></div>`)] : ''; })}`;
}
A.bibleInstall = ([n], el) => { n = decodeURIComponent(n); el.textContent = 'Downloading…'; el.disabled = true; setTimeout(() => { B().installed.push(n); if ($('biblesList')) put('biblesList', biblesList('')); toast(`${n} is installed`); }, 600); };
A.bibleRemove = ([n]) => { n = decodeURIComponent(n); B().installed = B().installed.filter(x => x !== n); B().rows.forEach(r => { if (r.bible === 'dl-' + n) r.bible = 'none'; }); put('biblesList', biblesList('')); render(); };

// A Bible from a file (as the app has it, 35b68c1): pick the file, then check its names; installing the same Bible again
// replaces the earlier copy.
A.bibleFromFile = () => { const inp = document.createElement('input'); inp.type = 'file'; inp.multiple = true; inp.accept = '.xml,.osis,.zef,.usfm,.sfm,.usx,.zip'; inp.onchange = () => bibleFiles([...inp.files].map(f => f.name)); inp.click(); };
// several files: each is read ("Reading <file>…"), then their sheets come one after another, the title saying how many wait
let bibleQueue = [];
function bibleFiles(names) { if (!names.length) return; bibleQueue = names.slice(1); toast(`Reading ${names[0]}…`); bibleFileSheet(names[0]); }
const bibleNext = () => { if (bibleQueue.length) { const n = bibleQueue.shift(); toast(`Reading ${n}…`); bibleFileSheet(n); return true; } return false; };
function bibleFileSheet(file) {
  const base = file.replace(/\.(xml|osis|zef|usfm|sfm|usx|zip)$/i, '').replace(/[_-]+/g, ' ').trim(), short = base.split(/\s+/).map(w => w[0] || '').join('').toUpperCase().slice(0, 4);
  const lang = /укр|ukr|ogienko|огієнк/i.test(base) ? 'uk' : /[а-яё]|rus|synod/i.test(base) ? 'ru' : 'en';
  openSheet({ title: `Install a Bible${bibleQueue.length ? ` · ${bibleQueue.length} more waiting` : ''}`, body: html`<p class="ui-p">From “${file}”. Check its names: the short name shows on slides and in the find list.</p>
    <label class="ui-f"><span>Name</span><input class="field" id="bfName" value="${base}"></label><label class="ui-f"><span>Short name</span><input class="field" id="bfShort" value="${short}"></label>
    <label class="ui-f"><span>Language</span><select class="sel" id="bfLang">${options([['en', 'English'], ['ru', 'Русский'], ['uk', 'Українська'], ['other', 'Another language']], lang)}</select></label>`,
    buttons: [{ t: 'Cancel', f: () => { if (!bibleNext()) A.biblesSheet(); return false; } }, { t: 'Install', primary: true, f: () => { const n = $('bfName').value.trim(); if (!n) { refuse('A Bible needs a name'); return false; }
      const inst = B().installed, again = inst.includes(n); if (!again) inst.push(n); toast(again ? `Replaced your earlier ${n}; the earlier copy is in the Trash` : `Installed ${n}`); if (!bibleNext()) A.biblesSheet(); return false; } }] });
}

/* ═════════ edit.js ═════════ */
// The editor, the app's way: a slide is objects; pick one, then move, resize or restyle it.
// The same editor works on a song's slide, a theme's template, a prop and a stage layout.
'use strict';

VIEWS.edit = { left: renderEditLeft, centre: renderEditCentre };
VIEWS.themes = { left: renderThemesLeft, centre: renderEditCentre };

/* ───────── what is being edited ───────── */
function edTarget() {
  if (state.mode === 'themes') {
    const th = byId(DOC.themes, state.themeSel.theme) || DOC.themes[0], ver = state.themeSel.ver || null, tpl = byId(th.templates, state.themeSel.tpl) || th.templates.find(x => (x.version || null) === ver) || th.templates[0];
    return { type: 'template', theme: th, tpl, holder: tpl, ctx: { raw: true, bg: th.picture ? (mediaById(th.picture) || {}).bg || th.bg : th.bg }, title: `${th.name} · ${tpl.version ? versionName(tpl.version) + ' · ' : ''}${tpl.name}` };
  }
  const t = state.edit.target || {};
  if (t.type === 'prop') { const p = byId(DOC.props, t.id) || DOC.props[0]; if (!p) return { type: 'prop', prop: null, holder: { objects: [] }, ctx: { raw: true, bg: '#151515' }, title: 'Props' };      // none yet: the empty list, with its Add key
    return { type: 'prop', prop: p, holder: p, ctx: { raw: true, bg: '#151515' }, title: p.name }; }
  if (t.type === 'layout') { const l = byId(DOC.stageLayouts, t.id) || DOC.stageLayouts[0]; return { type: 'layout', layout: l, holder: l, ctx: stageCtx(), title: l.name }; }
  const sg = songById(t.songId) || DOC.songs[0], k = Math.max(0, Math.min(t.k || 0, sg.slides.length - 1));
  return { type: 'slide', song: sg, k, slide: sg.slides[k], holder: sg.slides[k], ctx: { song: sg, kind: 'songs' }, title: sg.title };
}
const edEff = t => resolved(t.holder, t.ctx).objects;
const edPicked = t => t.holder.objects.filter(o => state.sels.includes(o.id));
function edIsLive(t) { if (t.type !== 'slide' || !state.live || !state.on) return false; const d = deckByKey(state.live.deck); return !!d && !!d.song && d.song.id === t.song.id && liveSlideIndex() === t.k; }

function enterEdit(deckKey, s, songId) {
  let sg = songId && songById(songId), k = s || 0;
  if (!sg && deckKey) { const d = deckByKey(deckKey); if (d && d.song) sg = d.song; }
  if (!sg) { const d = decks().find(x => x.key === (state.src.type === 'lib' ? 'lib-' + state.openSong : state.openItem)); if (d && d.song) { sg = d.song; k = 0; } }
  if (!sg) { const d = state.live && deckByKey(state.live.deck); if (d && d.song) { sg = d.song; k = liveSlideIndex(); } }
  if (!sg) { const d = decks().find(x => x.song); sg = d ? d.song : DOC.songs[0]; k = 0; }
  if (!sg) return refuse('There is no song to edit yet. Make one with New song.');
  state.edit.target = { type: 'slide', songId: sg.id, k: Math.max(0, k) }; state.mode = 'edit'; state.sels = []; state.reflow = false; render();
}
function editOther(type, id) { state.edit.target = { type, id }; state.mode = 'edit'; state.sels = []; state.reflow = false; render(); }

// Setting something on an object. On a slide it becomes the object's own; the rest still comes from the theme.
function setProp(o, eff, path, v) {
  const [a, b] = path.split('.');
  if (a === 'frame') o.frame = { ...eff.frame, [b]: v };
  else if (a === 'style') o.style = { ...(o.style || {}), [b]: v };
  else if (a === 'gradient') { if (b === 'kind' && !v) delete o.gradient; else o.gradient = { kind: 'linear', to: '#000000', angle: 90, ...(o.gradient || {}), [b]: v }; }
  else if (a === 'stroke') o.stroke = { w: 0, color: '#ffffff', ...(o.stroke || {}), [b]: v };
  else if (a === 'build') o.build = { in: 'None', from: 'Left', when: 'With the slide', delay: 0, out: 'None', byPara: false, ...(o.build || {}), [b]: v };
  else o[a] = v;
}
const getProp = (e, path) => path.split('.').reduce((t, k) => t == null ? undefined : t[k], e);
// A word's look (font, size, bold, italic, underlined, struck, colour) is set on the words picked in a box when there are some, else on
// the whole box: then every word takes it (the spans give that look up). One change, one Undo step, typing not yet kept included.
// perRun: each piece from its own value (A− and A+ keep the differences between sizes).
function edApply(path, fn, perRun = false) {
  const t = edTarget(), list = edPicked(t); if (!list.length) return;
  const key = path.startsWith('style.') && WORD_LOOK.includes(path.slice(6)) ? path.slice(6) : null;
  const el = typingEl(), typed = el && list.some(o => o.id === el.dataset.tx) ? el : null, sel = typed ? typedSelection(typed) : null;
  const w = !key ? null : typed ? (sel && sel.to > sel.from ? { id: typed.dataset.tx, from: sel.from, to: sel.to } : null) : wordsPicked();
  if (typed) state.retype = { id: typed.dataset.tx, from: sel ? sel.from : null, to: sel ? sel.to : null };
  if (w) state.tsel = { id: w.id, from: w.from, to: w.to };
  mutate(() => {
    const tt = edTarget(), eff = edEff(tt);
    if (typed) foldTyped(typed, tt, eff);
    if (w) return styleWords(tt, eff, w, key, fn, perRun);
    edPicked(tt).forEach(o => { const e = eff.find(x => x.id === o.id);
      if (!key || o.kind !== 'text') return setProp(o, e, path, fn(getProp(e, path), e));
      if (perRun) { setProp(o, e, path, fn(e.style[key])); if (o.spans) o.spans = o.spans.map(sp => sp[key] !== undefined ? { ...sp, [key]: fn(sp[key]) } : sp); }
      else { setProp(o, e, path, fn(wordsSum(o.text || '', o.spans, e.style, 0, (o.text || '').length)[key], e)); if (o.spans) o.spans = o.spans.map(sp => { const c = { ...sp }; delete c[key]; return c; }); }
      if (o.spans) { o.spans = tidySpans(o.text || '', o.spans, { ...e.style, ...o.style }); if (!o.spans.length) delete o.spans; }
    });
  });
}
A.edSet = ([path, v]) => edApply(path, () => v === '' ? null : v);
A.edToggle = ([path]) => edApply(path, now => now !== true);      // some on and some off: all on
A.edBump = ([path, by]) => edApply(path, now => Math.max(8, (now || 0) + +by), true);
C.edIn = (v, [path], el) => edApply(path, () => el.type === 'number' || el.type === 'range' ? +v : v === '' ? null : v);

/* ───────── words picked inside a box ───────── */
const MIXED = Symbol('mixed');
// The look of some of a box's words, prop by prop: one value, or MIXED (and, under prop + 's', every value there).
function wordsSum(text, spans, box, from, to) {
  const runs = runsOf(text, spans, [from, to]).filter(r => r.from >= from && r.to <= to && r.to > r.from), sum = {};
  for (const p of WORD_LOOK) { const vals = new Set((runs.length ? runs : [{ look: {} }]).map(r => r.look[p] !== undefined ? r.look[p] : box[p])); sum[p] = vals.size > 1 ? MIXED : [...vals][0]; sum[p + 's'] = [...vals]; }
  return sum;
}
function styleWords(tt, eff, w, key, fn, perRun) {
  const o = byId(tt.holder.objects, w.id), e = eff.find(x => x.id === w.id); if (!o || !e) return;
  const text = o.text || '', from = Math.min(w.from, text.length), to = Math.min(w.to, text.length); if (to <= from) return;
  const runs = runsOf(text, o.spans, [from, to]), inside = r => r.from >= from && r.to <= to, now = wordsSum(text, o.spans, e.style, from, to)[key];
  o.spans = tidySpans(text, runs.map(r => { const look = { ...r.look };
    if (inside(r)) { const v = perRun ? fn(look[key] !== undefined ? look[key] : e.style[key]) : fn(now); if (v == null || (key === 'size' && !(v >= 1))) delete look[key]; else look[key] = v; }
    return { from: r.from, to: r.to, ...look }; }), e.style);
  if (!o.spans.length) delete o.spans;
}
// The words picked in the box being styled, kept while the inspector is used (a number typed, a font chosen), checked against now.
function wordsPicked() {
  const w = state.tsel; if (!w || (state.mode !== 'edit' && state.mode !== 'themes') || state.sels.length !== 1 || state.sels[0] !== w.id) return null;
  const o = byId(edTarget().holder.objects, w.id); if (!o || o.kind !== 'text' || o.linked) return null;
  const to = Math.min(w.to, (o.text || '').length); return to > w.from ? { id: w.id, from: w.from, to } : null;
}
// What the inspector shows for a text box: the picked words' look, or the whole box's; mixed where they differ.
function wordsNow(e) {
  const el = typingEl(), mine = el && el.dataset.tx === e.id, now = mine ? readTyped(el, e.style) : { text: e.text || '', spans: e.linked ? [] : e.spans || [] };
  let w = null;
  if (mine) { const r = typedSelection(el); if (r && r.to > r.from) w = r; } else { const p = wordsPicked(); if (p && p.id === e.id) w = p; }
  if (w && w.to > now.text.length) w = null;
  return { w, text: now.text, sum: wordsSum(now.text, now.spans, e.style, w ? w.from : 0, w ? w.to : now.text.length) };
}

/* ───────── typing: the box's words read back with their looks ───────── */
const typingEl = () => { const a = document.activeElement; return a && a.isContentEditable && a.dataset && a.dataset.tx ? a : null; };
// Each piece of what is in a typed box, in order: its words (a line break as \n) and the look of the piece it is in. A bullet or
// number the box draws is passed over.
function walkTyped(root, visit) {
  let text = '';
  const go = (n, look) => { for (const c of n.childNodes) {
    if (c.nodeType === 3) { const t = c.data.replace(/\u00a0/g, ' '); visit(t, c, look); text += t; continue; }
    if (c.nodeType !== 1 || c.hasAttribute('data-pre')) continue;
    if (c.tagName === 'BR') { visit('\n', c, look); text += '\n'; continue; }
    if (/^(DIV|P)$/.test(c.tagName) && text && !text.endsWith('\n')) { visit('\n', c, look); text += '\n'; }      // a line some browsers make a block of its own
    let mine = look; if (c.hasAttribute('data-look')) { try { mine = JSON.parse(c.getAttribute('data-look')); } catch (_) { /* drawn here, so it always reads */ } }
    go(c, mine);
  } };
  go(root, null);
  return text;
}
// The words and their spans as typed. What is typed takes the look of the words before it (at the very start, after it).
function readTyped(el, box) {
  let text = ''; const looks = [];
  walkTyped(el, (t, n, look) => { text += t; for (let i = 0; i < t.length; i++) looks.push(look); });
  while (text.endsWith('\n')) { text = text.slice(0, -1); looks.pop(); }
  for (let i = 1; i < looks.length; i++) if (!looks[i]) looks[i] = looks[i - 1];
  const first = looks.find(Boolean) || {}; for (let i = 0; i < looks.length && !looks[i]; i++) looks[i] = first;
  const spans = []; looks.forEach((l, i) => { const last = spans[spans.length - 1]; if (last && last.to === i && sameLook(last, l)) last.to = i + 1; else spans.push({ from: i, to: i + 1, ...l }); });
  return { text, spans: tidySpans(text, spans, box) };
}
function typedIndex(el, node, off) {
  let pos = 0, at = null; const pt = document.createRange(); pt.setStart(node, off);
  walkTyped(el, (t, n) => { if (at != null) return; if (n === node && n.nodeType === 3) { at = pos + Math.min(off, t.length); return; } if (pt.comparePoint(n, 0) >= 0) { at = pos; return; } pos += t.length; });
  return at == null ? pos : at;
}
function typedSelection(el) {
  const s = getSelection(); if (!s.rangeCount) return null; const r = s.getRangeAt(0);
  if (!el.contains(r.startContainer) || !el.contains(r.endContainer)) return null;
  const a = typedIndex(el, r.startContainer, r.startOffset), b = typedIndex(el, r.endContainer, r.endOffset);
  return { from: Math.min(a, b), to: Math.max(a, b) };
}
function typedPoint(el, index) {
  let pos = 0, at = null;
  walkTyped(el, (t, n) => { if (at) return; if (n.nodeType === 3 && index <= pos + t.length) at = [n, index - pos]; else if (n.nodeType !== 3 && index === pos) at = [n.parentNode, [...n.parentNode.childNodes].indexOf(n)]; pos += t.length; });
  return at || [el, el.childNodes.length];
}
// Typing not yet kept, kept: true if anything changed.
function foldTyped(el, tt, eff) {
  const id = el.dataset.tx, o = byId(tt.holder.objects, id), e = eff.find(x => x.id === id); if (!o || !e) return false;
  const r = readTyped(el, e.style);
  if (r.text === (o.text || '') && JSON.stringify(r.spans) === JSON.stringify(tidySpans(o.text || '', o.spans, e.style))) return false;
  o.text = r.text; if (r.spans.length) o.spans = r.spans; else delete o.spans;
  return true;
}
document.addEventListener('selectionchange', () => {      // words picked or let go while typing: the inspector says what it now changes
  const el = typingEl(); if (!el) return;
  const r = typedSelection(el), next = r && r.to > r.from ? { id: el.dataset.tx, from: r.from, to: r.to } : null, was = state.tsel;
  if (next === was || (next && was && next.id === was.id && next.from === was.from && next.to === was.to)) return;
  state.tsel = next;
  if ((state.mode === 'edit' || state.mode === 'themes') && state.itab === 'Text') renderInspector();
});
// A style key pressed while typing keeps the words picked and the typing going.
document.addEventListener('mousedown', e => { if (typingEl() && e.target.closest && e.target.closest('#tabbody button')) e.preventDefault(); }, true);

/* ───────── one editor, four kinds of thing ───────── */
const edKindNow = () => state.mode === 'themes' ? 'themes' : ({ prop: 'props', layout: 'stage' }[(state.edit.target || {}).type] || 'slides');
const edKinds = () => html`<div class="ed-kinds">${seg([['slides', 'Slides'], ['themes', 'Themes'], ['props', 'Props'], ['stage', 'Stage']], edKindNow(), 'edKind')}</div>`;
A.edKind = ([k]) => {
  if (k === 'slides') return enterEdit();
  if (k === 'themes') { state.mode = 'themes'; state.sels = []; state.reflow = false; return render(); }
  if (k === 'props') return DOC.props.length ? editOther('prop', DOC.props[0].id) : A.propNew();
  editOther('layout', DOC.stageLayouts[0].id);
};
// From a slide, straight to the template that gives it its look.
A.edThemeOf = () => { const t = edTarget(); if (t.type !== 'slide') return; const th = themeFor(t.song), tpl = templateFor(t.slide, t.song, 'songs'); state.themeSel = { theme: th.id, tpl: tpl.id }; state.mode = 'themes'; state.sels = []; render(); };

/* ───────── left ───────── */
function objectsList(t) {
  const glyph = { text: 'T', shape: '▢', image: '▣', video: '▶', web: '◎', screen: '⧉' };
  const obs = [...t.holder.objects].reverse();
  return html`<div class="ed-objs"><div class="sect"><div class="sect-h"><span class="cap">Objects · front first</span><span class="sub ed-objn">${t.holder.objects.length}</span></div></div><div class="oblist">${obs.length ? obs.map(o => html`<div class="obrow ${state.sels.includes(o.id) ? 'on' : ''}"><span class="k">${o.linked ? '⟲' : glyph[o.kind]}</span>
    <button class="nm" data-a="edPick:${o.id}">${o.name}${o.role ? html`<i class="ed-role">${o.role}</i>` : ''}</button>
    <button class="tg ${o.hidden ? 'act' : ''}" data-a="edObjToggle:hidden,${o.id}" title="Hide or show">Hide</button><button class="tg ${o.locked ? 'act' : ''}" data-a="edObjToggle:locked,${o.id}" title="Lock or unlock">Lock</button></div>`) : html`<div class="sh-none">Nothing on it yet. Use Add above the slide.</div>`}</div></div>`;
}
function renderEditLeft() {
  const t = edTarget();
  if (t.type === 'prop') { put('left', edKinds(), html`<div class="sect"><div class="sect-h"><span class="cap">Props</span></div>${edActs('edPropOp', edWhy(DOC.props, t.prop, { none: 'Add a prop first', one: '' }))}<div class="edslides">${DOC.props.length ? DOC.props.map((p, n) => edTile(p === t.prop, `edOther:prop,${p.id}`, `prop:${p.id}`, thumb({ objects: p.objects }, { raw: true, bg: '#000000' }), n + 1, p.name)) : html`<p class="th-vnone">No props yet. Add makes one.</p>`}</div></div>${t.prop ? objectsList(t) : ''}`); return; }
  if (t.type === 'layout') { put('left', edKinds(), html`<div class="sect"><div class="sect-h"><span class="cap">Stage layouts</span></div>${edActs('edLayoutOp', edWhy(DOC.stageLayouts, t.layout, { none: 'No stage layout is open', one: 'There is always one stage layout' }))}<div class="edslides">${DOC.stageLayouts.map((l, n) => edTile(l === t.layout, `edOther:layout,${l.id}`, `layout:${l.id}`, html`<div class="thumb">${stageHTML(l.id)}</div>`, n + 1, l.name))}</div></div>${objectsList(t)}`); return; }
  const songs = [...new Map((state.src.type === 'pl' ? decks().filter(d => d.song).map(d => d.song) : librarySongs()).concat([t.song]).map(s => [s.id, s])).values()], one = t.song.slides.length === 1;
  put('left', edKinds(), html`<div class="sect"><div class="sect-h"><span class="cap">Song</span></div><div class="ed-songs">${songs.map(s => html`<button class="rowbtn ${s === t.song ? 'on' : ''}" data-a="edSong:${s.id}"><span class="ico ed-ico">${I.song}</span><span class="t">${s.title}</span></button>`)}</div></div>
    <div class="sect"><div class="sect-h"><span class="cap">Slides</span></div><div class="edkeys ed-five"><button class="btn" data-a="edSlideOp:add">Add</button><button class="btn" data-a="edSlideOp:dup">Duplicate</button><button class="btn" data-a="edSlideOp:del" ${{ disabled: one, title: one ? 'A song keeps at least one slide' : null }}>Delete</button><button class="btn" data-a="edSlideOp:up" title="Move up">↑</button><button class="btn" data-a="edSlideOp:down" title="Move down">↓</button></div></div>
    <div class="edslides">${t.song.slides.map((sl, n) => html`<button draggable="true" data-drag="edslide:${n}" class="edslide ${n === t.k ? 'on' : ''} ${edIsLive({ ...t, k: n }) ? 'live' : ''} ${sl.enabled ? '' : 'off'}" style="--g:${groupColour(sl.group)}" data-a="edSlideGo:${n}" data-m="edSlide:${n}">${thumb(sl, t.ctx)}<div class="strip"><span class="n">${two(n + 1)}</span><i class="dot"></i><span class="lbl"><span class="g">${sl.group || ''}</span>${sl.label ? ' · ' + sl.label : ''}</span></div></button>`)}</div>${objectsList(t)}`);
}
A.edSong = ([id]) => enterEdit(null, 0, id);
// With no group bars in this column, a slide's group is renamed on its strip (a double click, or Rename Group on its
// right-click menu), for every slide of that group in the song.
A.edGroupRename = ([n]) => { const t = edTarget(), sl = t.song.slides[+n], old = sl.group || '';
  renameAt(`.edslide[data-drag="edslide:${n}"] .lbl .g`, old, v => mutate(() => { t.song.slides.forEach(x => { if ((x.group || '') === old) x.group = v; }); if (t.song.arrangements) t.song.arrangements.forEach(a => { a.groups = a.groups.map(g => g === old ? v : g); }); }), 'Rename group'); };
M.edSlide = ([n]) => { const t = edTarget(), go = f => () => { state.edit.target.k = +n; f(); }; return [{ t: 'Rename Group', f: () => A.edGroupRename([n]) }, { t: 'Duplicate', f: go(() => A.edSlideOp(['dup'])) },
  { t: 'Delete…', dim: t.song.slides.length === 1, f: go(() => undoable('Delete this slide?', 'Undo brings it back.', 'Delete', () => A.edSlideOp(['del']))) }, { sep: true },
  { t: 'Move Up', dim: +n === 0, f: go(() => A.edSlideOp(['up'])) }, { t: 'Move Down', dim: +n === t.song.slides.length - 1, f: go(() => A.edSlideOp(['down'])) }]; };
A.edSlideGo = ([k]) => { state.edit.target.k = +k; state.sels = []; render(); };
A.edOther = ([type, id]) => editOther(type, id);
A.edSlideOp = ([what]) => {
  const t = edTarget(), sl = t.song.slides, k = t.k;
  if (what === 'del' && edIsLive(t)) return refuse('This slide is on screen. Clear it or move on first.');
  if (what === 'del' && sl.length === 1) return refuse('A song keeps at least one slide.');
  mutate(() => {
    const tt = edTarget(), list = tt.song.slides; state.sels = [];
    if (what === 'add' || what === 'dup') { const c = clone(list[k]); c.id = newId('s'); c.objects.forEach(o => { o.id = newId('o'); if (what === 'add' && o.kind === 'text' && !o.linked) { o.text = o.role === 'main' ? 'New slide' : ''; delete o.spans; } });
      if (what === 'add') { c.objects = c.objects.filter(o => o.role !== 'translation'); c.label = ''; c.notes = ''; c.hotkey = ''; c.actions = []; }
      list.splice(k + 1, 0, c); state.edit.target.k = k + 1; }
    if (what === 'del') { list.splice(k, 1); state.edit.target.k = Math.min(k, list.length - 1); }
    if (what === 'up' && k > 0) { list.splice(k - 1, 0, list.splice(k, 1)[0]); state.edit.target.k = k - 1; }
    if (what === 'down' && k < list.length - 1) { list.splice(k + 1, 0, list.splice(k, 1)[0]); state.edit.target.k = k + 1; }
  });
};
A.layoutNew = () => ask('New stage layout', 'Name', '', v => { mutate(() => { DOC.stageLayouts.push({ id: newId('lay'), name: v, objects: [link('Words', 'current', { x: 60, y: 60, w: 1800, h: 700 }, { size: 100, shadow: false })] }); }); editOther('layout', DOC.stageLayouts[DOC.stageLayouts.length - 1].id); });
M.layout = ([id]) => { const l = byId(DOC.stageLayouts, id); return [{ t: 'Rename…', f: () => ask('Rename layout', 'Name', l.name, v => mutate(() => { l.name = v; })) },
  { t: 'Duplicate', f: () => mutate(() => { const c = clone(l); c.id = newId('lay'); c.name += ' copy'; c.objects.forEach(o => { o.id = newId('o'); }); DOC.stageLayouts.push(c); }) },
  { t: 'Delete', dim: DOC.stageLayouts.length === 1, f: () => mutate(() => { DOC.stageLayouts.splice(DOC.stageLayouts.indexOf(l), 1); DOC.screens.forEach(s => { if (s.layout === id) s.layout = DOC.stageLayouts[0].id; }); state.edit.target.id = DOC.stageLayouts[0].id; }) }]; };

/* themes: the list of themes, then the templates of the one open */
// One look for Edit's left column in every view, the slides view's (the owner, 22 November: "i prefer the slides view across
// all edit views"): what is picked from as plain rows, then its parts as pictures with a strip, under a row of actions.
// A tile's caption: the name may end in "…" (it is content, 06 rule 6) with the whole caption in its tooltip; a kind after it
// ("· Songs", "· Bible") is not a name and never shrinks
const edTile = (on, a, m, pic, n, label, kind) => html`<button class="edslide ${on ? 'on' : ''}" data-a="${a}" data-m="${m}" draggable="true" data-drag="edtile:${m.replace(':', ',')}" title="${kind ? label + ' · ' + kind : label}">${pic}<div class="strip"><span class="n">${two(n)}</span>${kind ? html`<span class="lbl split"><span class="tn">${label}</span><span class="tk">· ${kind}</span></span>` : html`<span class="lbl">${label}</span>`}</div></button>`;
// greyed by context, with the reason as the hover (as the app has it, 0e15f1d)
const edActs = (op, why = {}) => { const k = (w, label, title) => html`<button class="btn" data-a="${op}:${w}" ${{ disabled: !!why[w], title: why[w] || title || null }}>${label}</button>`;
  return html`<div class="edkeys ed-five">${k('add', 'Add')}${k('dup', 'Duplicate')}${k('del', 'Delete')}${k('up', '↑', 'Move up')}${k('down', '↓', 'Move down')}</div>`; };
const edWhy = (list, item, last) => { const i = list.indexOf(item); if (!item) return { dup: last.none, del: last.none, up: last.none, down: last.none };
  return { del: list.length === 1 ? last.one : '', up: i === 0 ? 'Already at the top' : '', down: i === list.length - 1 ? 'Already at the bottom' : '' }; };
function renderThemesLeft() {
  const t = edTarget(), th = t.theme, ver = t.tpl.version || null, vers = DOC.versions.filter(v => th.templates.some(tp => tp.version === v.id)), set = th.templates.filter(tp => (tp.version || null) === ver);
  const forName = f => ({ songs: 'Songs', bible: 'Bible', both: 'Songs and Bible', none: 'By hand' })[f];
  put('left', edKinds(), html`<div class="sect"><div class="sect-h"><span class="cap">Theme</span><button class="iconbtn" data-a="themeNew" title="New theme">${I.plus}</button></div>
      ${DOC.themes.map(x => html`<button class="rowbtn ${x === th ? 'on' : ''}" data-a="thPick:${x.id}" data-m="theme:${x.id}" ${x.builtIn ? null : { draggable: 'true', 'data-drag': `edtile:theme,${x.id}` }}><span class="ico ed-ico">${I.theme}</span><span class="t">${x.name}</span>${x.id === DOC.mainTheme ? html`<span class="ed-tag">Main</span>` : ''}</button>`)}</div>
    <div class="sect"><div class="sect-h"><span class="cap">Templates</span></div>
      <div class="ed-vers">${vers.length ? seg([['', 'Usual'], ...vers.map(v => [v.id, v.name])], ver || '', 'thVer') : html`<span class="sub">Usual templates</span>`}<button class="btn sm" data-a="versionNew" title="Add a version to this theme: other templates for a moment in the service, such as the offering">${I.plus} Version</button></div>
      ${ver ? html`<p class="ed-verline">Used while the ${versionName(ver)} version is on, on every screen with this theme.<button class="iconbtn" data-a="menu:themeVersion,${ver}" aria-label="More for this version">${I.more}</button></p>` : ''}
      ${edActs('edTplOp', edWhy(set, t.tpl, { none: 'No template is open', one: ver ? '' : 'A theme always keeps one usual template' }))}<div class="edslides">${set.map((tp, n) => edTile(tp === t.tpl, `tplPick:${tp.id}`, `tpl:${tp.id}`, thumb({ objects: tp.objects }, { raw: true, bg: th.bg }), n + 1, tp.name, forName(tp.for)))}</div></div>${objectsList(t)}`);
}
A.thPick = ([id]) => { state.themeSel = { theme: id, tpl: null, ver: null }; state.sels = []; render(); };
A.thVer = ([v]) => { const th = edTarget().theme; state.themeSel.ver = v || null; state.themeSel.tpl = (th.templates.find(tp => (tp.version || null) === (v || null)) || {}).id || null; state.sels = []; render(); };
A.tplPick = ([id]) => { state.themeSel.tpl = id; state.sels = []; render(); };
A.themeNew = () => ask('New theme', 'Name', '', v => mutate(() => { const th = { id: newId('theme'), name: v, bg: '#000000', picture: null, templates: [twoLang(), oneLang(), bibleTpl()] }; DOC.themes.push(th); state.themeSel = { theme: th.id, tpl: null }; }));
A.tplNew = () => ask('New template', 'Name', '', v => mutate(() => { const th = edTarget().theme, tp = { id: newId('tpl'), name: v, for: 'none', objects: [box('Main', 'main', { x: 120, y: 240, w: 1680, h: 600 }, { size: 96 }, 'The words of the song')] }; th.templates.push(tp); state.themeSel.tpl = tp.id; }));
M.theme = ([id]) => { const th = byId(DOC.themes, id); return [
  { t: 'Make It the Main Theme', on: DOC.mainTheme === id, f: () => mutate(() => { DOC.mainTheme = id; }) },
  { t: 'Use On', sub: DOC.screens.filter(s => s.kind === 'Audience').map(s => DOC.mainTheme === id && !s.theme ? { t: `${s.name} · follows the main theme`, on: true, dim: true, f: () => {} } : { t: s.name, on: s.theme === id, f: () => mutate(() => { s.theme = s.theme === id ? null : id; }) }) },
  { t: 'Duplicate', f: () => mutate(() => { const c = clone(th); c.id = newId('theme'); c.name += ' copy'; delete c.builtIn; c.templates.forEach(tp => { tp.id = newId('tpl'); tp.objects.forEach(o => { o.id = newId('o'); }); }); DOC.themes.push(c); }) },
  { t: 'Rename…', f: () => ask('Rename theme', 'Name', th.name, v => mutate(() => { th.name = v; })) },
  { t: 'Delete…', dim: th.builtIn || DOC.themes.length === 1, f: () => confirmDo(`Delete the theme “${th.name}”?`, 'Songs and screens that used it take the main theme. This can’t be undone.', 'Delete', () => mutate(() => { DOC.themes.splice(DOC.themes.indexOf(th), 1); if (DOC.mainTheme === id) DOC.mainTheme = DOC.themes[0].id; DOC.screens.forEach(s => { if (s.theme === id) s.theme = null; }); DOC.songs.forEach(s => { if (s.theme === id) s.theme = null; }); state.themeSel = { theme: DOC.mainTheme, tpl: null }; })) }]; };
M.tpl = ([id]) => { const th = edTarget().theme, tp = byId(th.templates, id); return [
  { t: 'Used For', sub: [['songs', 'Songs'], ['bible', 'Bible'], ['both', 'Songs and Bible'], ['none', 'By hand']].map(([k, n]) => ({ t: n, on: tp.for === k, f: () => mutate(() => { tp.for = k; }) })) },
  { t: 'Duplicate', f: () => mutate(() => { const c = clone(tp); c.id = newId('tpl'); c.name += ' copy'; c.objects.forEach(o => { o.id = newId('o'); }); th.templates.push(c); }) },
  { t: 'Rename…', f: () => ask('Rename template', 'Name', tp.name, v => mutate(() => { tp.name = v; })) },
  { t: 'Delete', dim: th.templates.length === 1, f: () => mutate(() => { th.templates.splice(th.templates.indexOf(tp), 1); state.themeSel.tpl = null; }) }]; };

/* ───────── middle: the slide, full size ───────── */
function renderEditCentre() {
  const t = edTarget(), live = edIsLive(t), stale = live && screenIsStale();
  $('media').hidden = true; $('foot').hidden = true; $('slides').className = 'ed';
  const what = { slide: `Slide ${two((t.k || 0) + 1)}`, template: t.tpl && t.tpl.version ? `Template · ${versionName(t.tpl.version)} version` : 'Template', prop: 'Prop', layout: 'Stage layout' }[t.type];
  if (t.type === 'prop' && !t.prop) { put('slides', html`<div class="ed-bar"><b>Props</b></div><div class="sh-empty"><b>No props yet</b><span>A prop is something shown over the slides: a logo, a name, a lower third. Add makes one.</span><div><button class="btn primary" data-a="edPropOp:add">${I.plus} Add a prop</button></div></div>`); return; }
  const bar = keys => html`<div class="ed-bar"><b>${t.title}</b><span class="sub">${what}</span>${live ? html`<span class="livetag">ON SCREEN</span>` : ''}<span class="sp"></span>${keys}</div>`;
  if (state.reflow && t.type === 'slide') {
    put('slides', html`${bar()}<div class="reflow"><textarea id="reflowText" spellcheck="true">${songText(t.song)}</textarea>
      <div class="row"><span class="sub">[Group] starts a group · a blank line starts a slide · -- starts the translation</span><button class="btn" data-a="reflow:cancel">Cancel</button><button class="btn primary" data-a="reflow:apply">Rebuild slides</button></div></div><div class="ed-hint sub">The slides keep their looks, labels and notes</div>`);
    return;
  }
  const z = state.zoom, width = z === 'Fit' ? 'min(100%, calc((100vh - 250px) * 16 / 9))' : (1920 * +z / 100) + 'px';
  put('slides', html`${bar(html`<button class="btn" data-a="menu:edAdd">${I.plus} Add ▾</button>
      ${t.type === 'slide' ? html`<button class="btn" data-a="edWords" title="The song's words as text">Words</button>` : ''}<button class="iconbtn ui-ur" data-a="undo" aria-label="Undo" title="Undo (${keyText('⌘Z')})">${I.undo}</button><button class="iconbtn ui-ur" data-a="redo" aria-label="Redo" title="Redo (${keyText('⇧⌘Z')})">${I.redo}</button><i class="rule"></i>
      <select class="sel" data-c="edZoom" aria-label="Zoom">${options(['Fit', '25', '50', '75', '100', '150', '200'].map(x => [x, x === 'Fit' ? 'Fit' : x + '%']), z)}</select><button class="btn ${state.gridOn ? 'onkey' : ''}" data-a="edGrid" title="A grid that boxes snap to">Grid</button>
      ${t.type === 'slide' ? html`<i class="rule"></i><button class="btn ${state.liveEdit ? 'onkey' : ''}" data-a="edLive" title="Send changes to the screens as they are made">Live edit</button>` : ''}`)}
    <div class="ed-stage ${z === 'Fit' ? '' : 'zoomed'}"><div class="thumb canvas" id="canvas" style="width:${width}">${slideHTML(t.holder, t.ctx, true)}
      ${state.gridOn ? html`<i class="ed-grid"></i>` : ''}<i class="ed-safe"></i>${edRulers(t)}<i class="guide v"></i><i class="guide h"></i></div></div>
    <div class="ed-hint sub"><b class="ed-keys">Here the keyboard edits. Space and the arrows don’t run the show; click a slide or Clear All for that.</b>Click to pick · Shift-click picks more · drag to move · handles resize · click a picked text box again to type</div>
    ${stale ? html`<div class="stale"><span>The screen still shows the old version</span><button class="btn primary" data-a="edPush">Update screen</button></div>` : ''}`);
}
// Rulers in the slide's own pixels, with the picked object's extent marked.
function edRulers(t) {
  const e = edEff(t).filter(x => state.sels.includes(x.id)), tick = (list, per) => list.map(v => html`<i style="${per}:${per === 'left' ? v / 19.2 : v / 10.8}%">${v}</i>`);
  const L = e.length ? Math.min(...e.map(x => x.frame.x)) : 0, R = e.length ? Math.max(...e.map(x => x.frame.x + x.frame.w)) : 0, T = e.length ? Math.min(...e.map(x => x.frame.y)) : 0, B2 = e.length ? Math.max(...e.map(x => x.frame.y + x.frame.h)) : 0;
  return html`<div class="ed-ruler x">${tick([0, 200, 400, 600, 800, 1000, 1200, 1400, 1600, 1800], 'left')}${e.length ? html`<b style="left:${L / 19.2}%;width:${(R - L) / 19.2}%"></b>` : ''}</div>
    <div class="ed-ruler y">${tick([0, 200, 400, 600, 800, 1000], 'top')}${e.length ? html`<b style="top:${T / 10.8}%;height:${(B2 - T) / 10.8}%"></b>` : ''}</div>`;
}
C.edZoom = v => { state.zoom = v; render(); };
A.edGrid = () => { state.gridOn = !state.gridOn; render(); };
A.edWords = () => { state.reflow = true; state.sels = []; render(); };
A.edLive = () => { state.liveEdit = !state.liveEdit; if (state.liveEdit && edIsLive(edTarget())) putUp(); render(); };
A.edPush = () => { putUp(); render(); };
A.reflow = ([what]) => {
  if (what === 'cancel') { state.reflow = false; return render(); }
  const drafts = parseSongText($('reflowText').value); if (!drafts.length) return refuse('Type at least one line');
  mutate(() => { const t = edTarget(); t.song.slides = slidesFromDrafts(drafts, t.song.slides); state.edit.target.k = Math.min(t.k, drafts.length - 1); state.reflow = false; state.sels = []; });
};

/* adding objects */
M.edAdd = () => { const t = edTarget(); return [
  { t: 'Text', k: 'T', f: () => edInsert('text') },
  { t: 'Shape', sub: Object.entries(SHAPES).map(([k, n]) => ({ t: n, f: () => edInsert('shape', k) })) },
  { t: 'Picture or Video', sub: DOC.media.map(m => ({ t: m.name, f: () => edInsert('media', m.id) })) },
  { t: 'Web Page', f: () => edInsert('web', '') },
  { t: 'NDI Source', f: () => edInsert('ndi', null) },
  { t: 'Phone or iPad Screen', f: () => edInsert('mirror', null) },      // as built, 4f817ca: the screen of a device that shows it to this computer
  { t: 'Linked Text', sub: Object.entries(LINKS).map(([k, n]) => ({ t: n, f: () => edInsert('link', k) })) },
  t.type === 'layout' && { t: 'Screen Picture', sub: [{ t: 'Audience screens (main)', f: () => edInsert('screen', 'main') }, ...DOC.screens.filter(x => x.kind === 'Audience').map(x => ({ t: x.name, f: () => edInsert('screen', x.id) }))] },
  t.type === 'template' && { t: 'A Box for Words', sub: ['main', 'translation', 'verse1', 'ref1', 'verse2', 'ref2'].map(r => ({ t: r, f: () => edInsert('role', r) })) }]; };
// An NDI source is chosen as a web page's address is: in the inspector, from the sources the finder sees on the network (the
// app's own outputs aren't offered: one shown in a slide would show itself). The line under it says what is happening.
const ndiLine = o => { const st = ndiState(), found = NDI_SOURCES.filter(n => !n.own).length, src = ndiSource(o.source);
  if (st === 'none') return 'NDI isn’t installed on this computer. SoftPresenter uses the NDI Tools you install yourself, as it does for sending.';
  if (!o.source) return st === 'found' && found ? `${found} found on this network. The picture shows, live, while the slide is on screen.` : 'Looking for NDI on this network. A source shows here as soon as it starts sending.';
  if (!ndiHere(o.source)) return `“${src ? src.name : o.source}” isn’t sending now. On screen the slide shows without it until it is back.`;
  return 'The picture shows, live, while the slide is on screen.'; };
A.ndiLook = () => render();
function edInsert(kind, arg) {
  mutate(() => {
    const obs = edTarget().holder.objects; let o;
    // A new text box is named with its number on the slide ("Text 3"), in the language set at that moment, and keeps that
    // name; its sample words are drawn in the language set (as built, 24 November, 0f4542b)
    if (kind === 'text') o = obj('text', `Text ${obs.filter(x => x.kind === 'text').length + 1}`, { text: 'Text', frame: { x: 460, y: 440, w: 1000, h: 200 }, style: {} });
    if (kind === 'role') o = box(arg[0].toUpperCase() + arg.slice(1), arg, { x: 260, y: 400, w: 1400, h: 280 }, { size: 70 }, `Sample words for “${arg}”`);
    if (kind === 'link') o = link(LINKS[arg], arg, { x: 460, y: 80, w: 1000, h: 140 }, { size: 64, shadow: false }, arg === 'timer' ? { timer: DOC.timers[0] ? DOC.timers[0].name : '' } : {});
    if (kind === 'shape') o = obj('shape', SHAPES[arg], { shape: arg, fill: '#ff4a1a', frame: { x: 660, y: 390, w: 600, h: 300 } });
    if (kind === 'media') { const m = mediaById(arg); o = obj(m.video ? 'video' : 'image', m.name, { media: arg, scaling: 'Fill', frame: { x: 0, y: 0, w: 1920, h: 1080 } }); }
    if (kind === 'screen') o = obj('screen', 'Screen picture', { source: arg, frame: { x: 1160, y: 40, w: 720, h: 405 } });
    const same = k => obs.filter(x => x.kind === k).length, nth = (name, k) => same(k) ? `${name} ${same(k) + 1}` : name;      // a second one is "Web page 2", as the app names them
    if (kind === 'web') o = obj('web', nth('Web page', 'web'), { url: arg || '', frame: { x: 260, y: 200, w: 1400, h: 680 } });
    if (kind === 'mirror') o = obj('mirror', nth('Phone or iPad Screen', 'mirror'), { scaling: 'Fit', frame: { x: 260, y: 146, w: 1400, h: 788 } });
    if (kind === 'ndi') o = obj('ndi', nth('NDI source', 'ndi'), { source: arg || '', scaling: 'Fit', quality: 'Full', sound: 'Off', frame: { x: 260, y: 146, w: 1400, h: 788 } });
    if (kind === 'media') obs.unshift(o); else obs.push(o);        // a picture or video goes behind the words
    state.sels = [o.id]; state.itab = OWN_TAB[o.kind] || 'Box';
  });
  // A web page's address, or an NDI source, is chosen in the inspector, which takes the keyboard at once (as in the app)
  if (kind === 'web' || kind === 'ndi') setTimeout(() => { const f = document.querySelector(`[data-c="edIn:${kind === 'web' ? 'url' : 'source'}"]`); if (f) f.focus(); });
}
A.edPick = ([id], el, ev) => { state.tsel = null; if (ev && ev.shiftKey) state.sels.includes(id) ? state.sels.splice(state.sels.indexOf(id), 1) : state.sels.push(id); else state.sels = [id]; const o = edPicked(edTarget())[0]; if (o && !['Box', 'Animation'].includes(state.itab)) state.itab = OWN_TAB[o.kind] || 'Box'; render(); };      // Box and Animation stay open from one object to the next
A.edObjToggle = ([prop, id]) => mutate(() => { const o = byId(edTarget().holder.objects, id); o[prop] = !o[prop]; });
A.edObj = ([what]) => {
  const t = edTarget(), picked = edPicked(t); if (!picked.length) return;
  if (what === 'rename') return ask('Rename object', 'Name', picked[0].name, v => mutate(() => { edPicked(edTarget())[0].name = v; }));
  if (what === 'copy') { state.clip = { type: 'objects', objects: clone(picked), eff: clone(edEff(t).filter(e => state.sels.includes(e.id))) }; return toast(`${picked.length} object${picked.length > 1 ? 's' : ''} copied`); }
  mutate(() => {
    const obs = edTarget().holder.objects, sel = edPicked(edTarget());
    for (const o of sel) { const n = obs.indexOf(o);
      if (what === 'del') obs.splice(n, 1);
      if (what === 'front' && n < obs.length - 1) { obs.splice(n, 1); obs.splice(n + 1, 0, o); }
      if (what === 'back' && n > 0) { obs.splice(n, 1); obs.splice(n - 1, 0, o); } }
    if (what === 'del') state.sels = [];
    if (what === 'dup') state.sels = sel.map(o => { const e = edEff(t).find(x => x.id === o.id), c = clone(o); c.id = newId('o'); c.name += ' copy'; delete c.role; c.frame = { ...e.frame, x: e.frame.x + 40, y: e.frame.y + 40 }; if (o.kind === 'text') c.style = { ...e.style }; obs.push(c); return c.id; });
  });
};
A.edPaste = () => { if (!state.clip || state.clip.type !== 'objects') return; mutate(() => { const obs = edTarget().holder.objects; state.sels = state.clip.objects.map((o, n) => { const e = state.clip.eff[n], c = clone(o); c.id = newId('o'); delete c.role; c.frame = { ...e.frame, x: e.frame.x + 40, y: e.frame.y + 40 }; if (c.kind === 'text') c.style = { ...e.style }; obs.push(c); return c.id; }); }); };
A.edAlign = ([how]) => {
  const t = edTarget(), eff = edEff(t).filter(e => state.sels.includes(e.id)); if (!eff.length) return;
  const many = eff.length > 1, L = many ? Math.min(...eff.map(e => e.frame.x)) : 0, R = many ? Math.max(...eff.map(e => e.frame.x + e.frame.w)) : 1920, T = many ? Math.min(...eff.map(e => e.frame.y)) : 0, Bt = many ? Math.max(...eff.map(e => e.frame.y + e.frame.h)) : 1080;
  mutate(() => {
    const obs = edPicked(edTarget()), fr = o => eff.find(e => e.id === o.id).frame, put = (o, f) => { o.frame = { ...fr(o), ...f }; };
    if (how === 'spreadh' || how === 'spreadv') { const ax = how === 'spreadh' ? ['x', 'w'] : ['y', 'h'], sorted = [...obs].sort((a, b) => fr(a)[ax[0]] - fr(b)[ax[0]]), total = sorted.reduce((n, o) => n + fr(o)[ax[1]], 0), lo = how === 'spreadh' ? L : T, hi = how === 'spreadh' ? R : Bt, gap = (hi - lo - total) / Math.max(1, sorted.length - 1); let at = lo;
      sorted.forEach(o => { put(o, { [ax[0]]: Math.round(at) }); at += fr(o)[ax[1]] + gap; }); return; }
    obs.forEach(o => { const f = fr(o);
      if (how === 'left') put(o, { x: L }); if (how === 'right') put(o, { x: R - f.w }); if (how === 'center') put(o, { x: Math.round((L + R) / 2 - f.w / 2) });
      if (how === 'top') put(o, { y: T }); if (how === 'bottom') put(o, { y: Bt - f.h }); if (how === 'middle') put(o, { y: Math.round((T + Bt) / 2 - f.h / 2) }); });
  });
};

/* ───────── inspector: where the timers sit in Service ───────── */
// The inspector's tabs follow what is picked (the owner, 23 November: "instead of naming it shape … name it to something that
// makes sense"): the object's own tab first, named for what it is; then Box (where it sits, its size, the box's look, its order,
// rename, duplicate, delete); then Animation, on slides and props only. Several of one kind keep their kind's tab; several of
// different kinds share Box (and Animation).
const OWN_TAB = { text: 'Text', shape: 'Shape', image: 'Picture', video: 'Video', web: 'Web Page', ndi: 'NDI Source', mirror: 'Phone or iPad Screen', screen: 'Screen' };
const inspTabs = (t, eff) => { const kinds = [...new Set(eff.map(e => e.kind))], own = kinds.length === 1 ? OWN_TAB[kinds[0]] : null;
  return [own, 'Box', t.type === 'slide' || t.type === 'prop' ? 'Animation' : null].filter(Boolean); };
function renderInspector() {
  const t = edTarget(), eff = edEff(t).filter(e => state.sels.includes(e.id)), o = eff[0];
  const none = { slide: 'Slide', template: 'Theme', prop: 'Prop', layout: 'Layout' }[t.type];
  const tabs = !o ? [none] : inspTabs(t, eff);
  if (!tabs.includes(state.itab)) state.itab = tabs[0];
  $('tabs').className = 'tabs' + (tabs.length === 1 ? ' one' : '');
  put('tabs', tabs.map(x => html`<button class="${x === state.itab ? 'on' : ''}" data-a="itab:${x}">${x}</button>`));
  const row = (label, ctl) => html`<div class="irow"><span class="cap">${label}</span><div class="ctl">${ctl}</div></div>`;
  const key = (face, on, a, title = '', mixed = false) => html`<button class="btn sq ${on ? 'onkey' : mixed ? 'mixed' : ''}" data-a="${a}" title="${mixed ? title + ': some are, some aren’t' : title}">${face}</button>`;
  // mixed: the words picked have several colours; none is ringed and the last well shows two of them
  const sw = (path, now, noneToo, mixed = null) => html`${noneToo ? html`<button class="swt none ${!now ? 'on' : ''}" data-a="edSet:${path}," title="None"></button>` : ''}${SWATCHES.map(c => html`<button class="swt ${now === c ? 'on' : ''}" style="background:${c}" data-a="edSet:${path},${c}" title="${c}"></button>`)}<label class="swt any ${now && !SWATCHES.includes(now) ? 'on' : ''}${mixed ? ' mixed' : ''}" title="${mixed ? 'Mixed colours: choose one for all' : 'Any colour'}" style="${mixed ? `background:linear-gradient(135deg, ${mixed[0]} 50%, ${mixed[1]} 50%)` : now && !SWATCHES.includes(now) ? 'background:' + now : ''}"><input type="color" data-c="edIn:${path}" value="${/^#[0-9a-f]{6}$/i.test(now || '') ? now : '#808080'}" aria-label="Any colour"></label>`;
  const sel = (path, list, now) => html`<select class="sel" data-c="edIn:${path}">${options(list, now == null ? '' : now)}</select>`;
  const num = (path, v, more = null) => html`<input class="num" type="number" data-c="edIn:${path}" value="${v}" ${more}>`;
  let body = '';
  if (!o) {
    if (t.type === 'slide') { const s = t.slide, th = themeFor(t.song);
      body = [row('Group', html`<select class="sel" data-c="edSlide:group">${options([...new Set([s.group, ...DOC.groups.map(g => g.name)])], s.group)}</select>`),
        row('Label', html`<select class="sel" data-c="edSlide:label">${options([['', 'None'], ...new Set([...LABELS, s.label].filter(Boolean))], s.label)}</select>`),
        row('Template', html`<select class="sel" data-c="edSlide:template">${options([['', 'Usual (the theme picks)'], ...th.templates.filter(x => x.for !== 'bible').map(x => [x.id, x.name])], s.template || '')}</select>`),
        row('Go to next', html`<select class="sel" data-c="edSlide:advance">${options([0, 3, 5, 8, 15, 30].map(n => [n, n ? `After ${n} seconds` : 'Off']), s.advance || 0)}</select>`),
        html`<div><span class="cap">Slide notes</span><textarea data-c="edSlide:notes" placeholder="For the operator and the stage screen" style="margin-top:6px">${s.notes}</textarea></div>
        <div class="hint">Words take their place and look from the theme’s template until a box is moved or restyled here. Pick an object to change it.</div>
        <button class="btn" data-a="edThemeOf" style="justify-self:start">Edit this slide’s template: ${themeFor(t.song).name} · ${templateFor(s, t.song, 'songs').name}</button>`];
    }
    if (t.type === 'template') body = [row('Used for', html`<select class="sel" data-c="tplFor">${options([['songs', 'Songs'], ['bible', 'Bible'], ['both', 'Both'], ['none', 'Nothing by itself']], t.tpl.for)}</select>`),
      row('Behind', SWATCHES.map(c => html`<button class="swt ${t.theme.bg === c ? 'on' : ''}" style="background:${c}" data-a="thSet:bg,${c}"></button>`)),
      row('Picture', html`<select class="sel" data-c="thPicture">${options([['', 'None'], ...DOC.media.filter(m => !m.video).map(m => [m.id, m.name])], t.theme.picture || '')}</select>`),
      html`<div class="hint">The colour and picture show behind every slide of this theme while no media is on. A template’s boxes say where each kind of words goes; its shapes and pictures are drawn behind the words.</div>`];
    if (t.type === 'prop' && !t.prop) body = '';
    else if (t.type === 'prop') body = [row('Clears', html`<select class="sel" data-c="propClears:${t.prop.id}">${options([0, 10, 30, 60].map(n => [n, n ? `After ${n} seconds` : 'Never']), t.prop.clears || 0)}</select>`), html`<div class="hint">A prop stays over the slides until it is taken off. Make it here like a slide.</div>`];
    if (t.type === 'layout') body = html`<div class="hint">Each box shows something live: the words on screen, the next slide, the clock, a timer, the notes. Add more from Add › Linked Text. Screens using this layout: ${DOC.screens.filter(s => s.layout === t.layout.id).map(s => s.name).join(', ') || 'none'}.</div>`;
  } else if (state.itab === 'Text') { const s = o.style, wn = wordsNow(o), w = wn.w, v = wn.sum, mx = p => v[p] === MIXED;
    const said = w ? wn.text.slice(w.from, w.to).replace(/\s+/g, ' ').trim() : '', quote = said.length > 26 ? said.slice(0, 25).trimEnd() + '…' : said;
    // The line that says what the four rows under it change: the words picked, or the whole box (and then every word in it).
    const scope = (label, note) => html`<div class="iscope"><span class="cap">${label}</span>${note ? html`<span class="sub">${note}</span>` : ''}</div>`;
    body = [scope(w ? 'Selected words' : eff.length > 1 ? `${eff.length} boxes` : 'The whole box', w ? `“${quote}”` : eff.length > 1 || o.linked ? '' : 'Select words in it to change only them'),
      row('Font', html`<button class="btn ed-font ${mx('font') ? 'mixed' : ''}" data-a="fontSheet" style="${mx('font') ? '' : 'font-family:' + FONTS[v.font]}">${mx('font') ? 'Mixed' : v.font}</button>`),
      row('Size', [num('style.size', mx('size') ? '' : v.size, { min: '8', max: '400', placeholder: mx('size') ? 'Mixed' : null, class: mx('size') ? 'num mixed' : null, 'aria-label': 'Size' }), key('A−', false, 'edBump:style.size,-4', 'Smaller'), key('A+', false, 'edBump:style.size,4', 'Bigger')]),
      row('Style', [key('B', v.bold === true, 'edToggle:style.bold', 'Bold (⌘B)', mx('bold')), key(html`<i>I</i>`, v.italic === true, 'edToggle:style.italic', 'Italic (⌘I)', mx('italic')), key(html`<u>U</u>`, v.underline === true, 'edToggle:style.underline', 'Underlined (⌘U)', mx('underline')), key(html`<s>S</s>`, v.strike === true, 'edToggle:style.strike', 'Struck through', mx('strike'))]),
      row('Colour', sw('style.color', mx('color') ? null : v.color, false, mx('color') ? v.colors.slice(0, 2) : null)),
      w ? scope('The whole box', '') : '',
      row('Align', [['left', 'center', 'right'].map(a => key(I[a], s.align === a, `edSet:style.align,${a}`, a)), html`<i class="rule"></i>`, ['top', 'middle', 'bottom'].map(a => key(I[a], s.valign === a, `edSet:style.valign,${a}`, a))]),
      row('Effects', html`<button class="btn ${s.stroke ? 'onkey' : ''}" data-a="edToggle:style.stroke">Outline</button><button class="btn ${s.shadow ? 'onkey' : ''}" data-a="edToggle:style.shadow">Shadow</button>`),
      s.shadow ? row('Shadow', html`<label class="ed-mini">Angle${num('style.shadowAngle', s.shadowAngle, { step: '15' })}</label><label class="ed-mini">Offset${num('style.shadowOffset', s.shadowOffset, { min: '0' })}</label><label class="ed-mini">Blur${num('style.shadowBlur', s.shadowBlur, { min: '0' })}</label>`) : '',
      row('Margins', [num('style.margin', s.margin, { min: '0', max: '300', step: '10' }), html`<span class="sub">inside the box</span>`]),
      row('Spacing', html`<label class="ed-mini">Letters${num('style.spacing', s.spacing, { step: '1' })}</label><label class="ed-mini">Lines${num('style.lineHeight', s.lineHeight, { step: '0.05', min: '0.8', max: '3' })}</label>`),
      row('Capitals', sel('style.caps', [['none', 'As typed'], ['upper', 'ALL CAPITALS'], ['lower', 'all small'], ['title', 'Each Word']], s.caps)),
      row('Fitting', sel('style.scaling', ['None', 'Text down', 'Text up', 'Text up or down', 'Box up or down'], s.scaling)),
      row('Lines', [sel('style.bullets', [['none', 'Plain'], ['bullets', 'Bullets'], ['numbers', 'Numbers']], s.bullets), sel('style.scroll', [['none', 'Still'], ['left', 'Scrolls left'], ['right', 'Scrolls right'], ['up', 'Scrolls up'], ['down', 'Scrolls down']], s.scroll)]),
      row('Behind', sw('style.bg', s.bg, true)),
      row('Shows', html`<select class="sel" data-c="edIn:linked">${options([['', 'Its own words'], ...Object.entries(LINKS)], o.linked || '')}</select>${o.linked === 'timer' ? html`<select class="sel" data-c="edIn:timer">${options(DOC.timers.map(x => x.name), o.timer)}</select>` : ''}`),
      t.type === 'template' ? row('Holds', html`<select class="sel" data-c="edIn:role">${options([['', 'Nothing (just text)'], 'main', 'translation', 'verse1', 'ref1', 'verse2', 'ref2'], o.role || '')}</select>`) : '',
      html`<div class="ctl"><button class="btn" data-a="edStyle:copy">Copy style</button><button class="btn" data-a="edStyle:paste">Paste style</button>${t.type === 'slide' && o.role ? html`<button class="btn" data-a="edStyle:theme" title="Let the theme decide the box’s look and place again; words styled in it keep their own">Back to theme</button>` : ''}${o.spans && o.spans.length && !o.linked ? html`<button class="btn" data-a="edStyle:plain" title="Every word takes the box’s look again">Clear word styles</button>` : ''}</div>`];
  } else if (state.itab === 'Box') { const f = o.frame, g = o.gradient, st = o.stroke || { w: 0, color: '#ffffff' }, kinds = [...new Set(eff.map(e => e.kind))], words = kinds.every(k => k === 'text');
    const what = { text: 'text box', shape: 'shape', image: 'picture', video: 'video', web: 'web page', ndi: 'NDI source', screen: 'screen picture' };
    body = [eff.length > 1 ? html`<div class="iscope"><span class="cap">${eff.length} objects</span><span class="sub">${kinds.map(k => what[k] || k).join(', ')}</span></div>` : '',
      row('Place', html`<div class="num4">${['x', 'y', 'w', 'h'].map(p => html`<label>${p.toUpperCase()}${num('frame.' + p, Math.round(f[p]))}</label>`)}</div>`),
      row('Arrange', [['left', 'center', 'right', 'top', 'middle', 'bottom'].map(a => key(I[a], false, `edAlign:${a}`, `Align ${a}${eff.length > 1 ? '' : ' on the slide'}`)), eff.length > 2 ? html`<button class="btn" data-a="edAlign:spreadh">Spread ↔</button><button class="btn" data-a="edAlign:spreadv">Spread ↕</button>` : '']),
      row('Rotate', [num('rotation', o.rotation || 0, { step: '5', min: '-180', max: '180' }), html`<span class="sub">degrees</span>`]),
      row('Opacity', html`<input type="range" min="0" max="1" step="0.05" value="${o.opacity == null ? 1 : o.opacity}" data-c="edIn:opacity" aria-label="Opacity" style="flex:1">`),
      words ? [row('Fill', sw('fill', o.fill, true)), row('Gradient', html`<select class="sel" data-c="edIn:gradient.kind">${options([['', 'None'], ['linear', 'Linear'], ['radial', 'Radial']], g ? g.kind : '')}</select>${g && g.kind === 'linear' ? num('gradient.angle', g.angle, { step: '15' }) : ''}`), g ? row('To', sw('gradient.to', g.to)) : ''] : '',
      kinds.includes('shape') ? '' : row('Outline', [num('stroke.w', st.w, { min: '0', max: '60' }), SWATCHES.slice(0, 5).map(c => html`<button class="swt ${st.color === c ? 'on' : ''}" style="background:${c}" data-a="edSet:stroke.color,${c}"></button>`)]),
      row('Effects', html`<button class="btn ${o.shadow ? 'onkey' : ''}" data-a="edToggle:shadow">Shadow</button><button class="btn ${o.flipH ? 'onkey' : ''}" data-a="edToggle:flipH">Flip ↔</button><button class="btn ${o.flipV ? 'onkey' : ''}" data-a="edToggle:flipV">Flip ↕</button>`),
      row('Order', html`<button class="btn" data-a="edObj:front">Forward</button><button class="btn" data-a="edObj:back">Back</button>`),
      row('Object', html`<button class="btn" data-a="edObj:rename">Rename</button><button class="btn" data-a="edObj:dup">Duplicate</button><button class="btn" data-a="edObj:del">Delete</button>`)];
  } else if (state.itab !== 'Animation') { const g = o.gradient, st = o.stroke || { w: 0, color: '#ffffff' }, m = (o.kind === 'image' || o.kind === 'video') && mediaById(o.media);      // the object's own tab
    body = o.kind === 'shape' ? [row('Shape', sel('shape', Object.entries(SHAPES), o.shape)), row('Fill', sw('fill', o.fill, true)), row('Gradient', html`<select class="sel" data-c="edIn:gradient.kind">${options([['', 'None'], ['linear', 'Linear'], ['radial', 'Radial']], g ? g.kind : '')}</select>${g && g.kind === 'linear' ? num('gradient.angle', g.angle, { step: '15' }) : ''}`), g ? row('To', sw('gradient.to', g.to)) : '',
        row('Outline', [num('stroke.w', st.w, { min: '0', max: '60' }), SWATCHES.slice(0, 5).map(c => html`<button class="swt ${st.color === c ? 'on' : ''}" style="background:${c}" data-a="edSet:stroke.color,${c}"></button>`)])]
      : o.kind === 'image' || o.kind === 'video' ? [row('File', html`<span class="ed-file" title="${m ? m.name : ''}">${m ? m.name : 'Gone from the media bin'}</span>`), row('Fits', sel('scaling', ['Fit', 'Fill', 'Stretch'], o.scaling || 'Fill')),
        o.kind === 'video' ? html`<p class="ed-line">It plays from the start while the slide is on screen.</p>` : '']
      : o.kind === 'mirror' ? [html`<p class="ed-line ${mirrorSet().on ? '' : 'care'}">${!mirrorSet().on ? 'Showing a phone’s screen is switched off. Switch it on in Settings › Phone Screens.' : state.mirrorDevice ? `${state.mirrorDevice} is showing its screen. It shows here, live, while the slide is on screen.` : `Nobody is showing a screen yet. On the iPhone or iPad, open Control Centre, tap Screen Mirroring and choose ${mirrorSet().name}.`}</p>`,
        row('Fits', sel('scaling', ['Fit', 'Fill', 'Stretch'], o.scaling || 'Fit'))]
      : o.kind === 'ndi' ? [row('Source', html`<select class="sel" data-c="edIn:source">${options([['', ndiState() === 'none' ? 'NDI isn’t installed' : ndiState() === 'found' && NDI_SOURCES.some(n => !n.own) ? 'Choose a source…' : 'Looking…'], ...(ndiState() === 'found' ? NDI_SOURCES.filter(n => !n.own).map(n => [n.id, `${n.name} · ${n.host}`]) : []), ...(o.source && !ndiHere(o.source) ? [[o.source, `${ndiSource(o.source) ? ndiSource(o.source).name : o.source} · not sending`]] : [])], o.source || '')}</select><button class="btn" data-a="ndiLook" title="Look for NDI sources again">Look again</button>`),
        html`<p class="ed-line ${o.source && !ndiHere(o.source) && ndiState() !== 'none' ? 'care' : ''}">${ndiLine(o)}</p>`,
        row('Fits', sel('scaling', ['Fit', 'Fill', 'Stretch'], o.scaling || 'Fit')),
        row('Quality', html`${sel('quality', ['Full', 'Light'], o.quality || 'Full')}<span class="sub">Light takes the sender’s smaller picture</span>`),
        row('Sound', html`<select class="sel" data-c="edIn:sound" disabled>${options(['Off'], 'Off')}</select><span class="sub">Sound comes later</span>`)]      // as the app has it (de5bc10): no way yet to take NDI's sound
      : o.kind === 'web' ? [row('Address', html`<input class="field" data-c="edIn:url" placeholder="https://…" value="${o.url == null ? '' : o.url}">`),
        html`<p class="ed-line ${o.url && !/^https:\/\//.test(o.url) ? 'care' : ''}">${o.url && !/^https:\/\//.test(o.url) ? 'Only web pages whose address starts with https:// can be shown.' : 'The page shows, live, while the slide is on screen.'}</p>`]
      : o.kind === 'screen' ? [row('Shows', html`<select class="sel" data-c="edIn:source">${options([['main', 'Audience screens (main)'], ...DOC.screens.filter(x => x.kind === 'Audience').map(x => [x.id, x.name])], o.source)}</select>`), html`<p class="ed-line">A live picture of that screen, for the stage.</p>`]
      : '';
  } else { const b = { in: 'None', from: 'Left', when: 'With the slide', delay: 0, out: 'None', byPara: false, ...(o.build || {}) };
    body = [row('Comes on', [sel('build.in', ['None', 'Dissolve', 'Fly In', 'Zoom'], b.in), b.in === 'Fly In' ? sel('build.from', ['Left', 'Right', 'Top', 'Bottom'], b.from) : '']),
      row('When', sel('build.when', ['With the slide', 'On a click of Next'], b.when)), row('Delay', [num('build.delay', b.delay, { min: '0', max: '10', step: '0.5' }), html`<span class="sub">seconds</span>`]),
      row('Goes off', sel('build.out', ['None', 'Dissolve', 'Fly Out', 'Zoom'], b.out)),
      o.kind === 'text' ? row('Text', html`<button class="btn ${b.byPara ? 'onkey' : ''}" data-a="edToggle:build.byPara">By paragraph</button>`) : '',
      html`<div class="hint">Next sets a slide’s click builds going in order before it moves on; Previous takes them back. They play in the preview on the right when the slide goes up.</div>`];
  }
  put('tabbody', html`<div class="insp">${body}</div>`);
}
A.itab = ([x]) => { state.itab = x; renderRight(); };
// One html piece, not a bare list: main.js's font search still draws #fontList from it as a string, through the bridge.
const fontNames = () => [DEF_FONT, ...Object.keys(FONTS).filter(n => n !== DEF_FONT).sort((a, b) => a.localeCompare(b))];      // the app's order: its own face first, then the computer's A–Z
const fontRows = f => { const names = fontNames().filter(n => n.toLowerCase().includes(f.toLowerCase()));
  return names.length ? html`${names.map(n => html`<button class="rowbtn" data-a="fontPick:${n}"><span class="t" style="font-family:${FONTS[n]};font-size:17px">${n}</span></button>`)}` : html`<p class="ui-p sub">No font by that name</p>`; };
A.fontSheet = () => openSheet({ title: 'Font', body: html`<input class="field" id="fontFind" data-i="fontFind" placeholder="Search fonts"><div class="ui-list" id="fontList">${fontRows('')}</div>`, buttons: [{ t: 'Cancel' }] });
A.fontPick = ([n]) => { closeSheet(); edApply('style.font', () => n); };
C.edSlide = (v, [prop]) => { if (prop === 'notes') { edTarget().slide.notes = v; return; } mutate(() => { const s = edTarget().slide; s[prop] = prop === 'advance' ? +v : v || (prop === 'template' ? null : ''); }); };
C.tplFor = v => mutate(() => { edTarget().tpl.for = v; });
A.thSet = ([prop, v]) => mutate(() => { edTarget().theme[prop] = v; });
C.thPicture = v => mutate(() => { edTarget().theme.picture = v || null; });
C.propClears = (v, [id]) => mutate(() => { byId(DOC.props, id).clears = +v; });
A.edStyle = ([what]) => {
  const t = edTarget(), e = edEff(t).find(x => state.sels.includes(x.id)); if (!e || e.kind !== 'text') return;
  if (what === 'copy') { state.clip = { type: 'style', style: clone(e.style) }; return toast('Text style copied'); }
  if (what === 'paste' && (!state.clip || state.clip.type !== 'style')) return refuse('Copy a style first');
  if (what === 'plain') { state.tsel = null; return mutate(() => edPicked(edTarget()).forEach(o => { delete o.spans; })); }
  mutate(() => edPicked(edTarget()).forEach(o => { if (what === 'theme') { delete o.style; delete o.frame; } else if (o.kind === 'text') o.style = { ...(o.style || {}), ...clone(state.clip.style) }; }));
};

/* ───────── typing in a text box: kept when the box loses focus ───────── */
// from, to: the words to pick again (after a style was set while typing); none: the caret at the end.
function startTyping(id, from, to) {
  const el = document.querySelector(`#canvas [data-tx="${CSS.escape(id)}"]`); if (!el) return;
  el.setAttribute('contenteditable', 'plaintext-only'); el.focus();
  const r = document.createRange();
  if (from == null) { r.selectNodeContents(el); r.collapse(false); }
  else { const x = typedPoint(el, from), y = typedPoint(el, to == null ? from : to); r.setStart(x[0], x[1]); r.setEnd(y[0], y[1]); }
  const s = getSelection(); s.removeAllRanges(); s.addRange(r);
}
document.addEventListener('focusout', e => {
  const el = e.target, id = el.dataset && el.dataset.tx; if (!id || !el.hasAttribute('contenteditable')) return;
  el.removeAttribute('contenteditable');
  if (drawing) return;      // taken away by a redraw: what was typed was kept just before it
  const keep = [e.relatedTarget, pointerEl].some(n => n && n.closest && n.closest('aside.right, #sheet, #menu'));      // to the inspector or its font list: the words stay picked
  if (!keep) state.tsel = null;
  if (!el.isConnected) return;
  if (el.dataset.cancel) { delete el.dataset.cancel; state.tsel = null; return render(); }      // Escape: back as it was
  const t = edTarget(), o = byId(t.holder.objects, id), ef = edEff(t).find(x => x.id === id); if (!o || !ef) return;
  const r = readTyped(el, ef.style), same = r.text === (o.text || '') && JSON.stringify(r.spans) === JSON.stringify(tidySpans(o.text || '', o.spans, ef.style));
  if (keep && state.tsel) {      // to the inspector with words selected: what was typed is kept (one Undo step) and only the slide is redrawn, with the
    // words highlighted, so the field or key being pressed stays where it is
    if (!same) { undoStack.push(JSON.stringify(DOC)); redoStack = []; foldTyped(el, t, edEff(t)); fixState(); if (state.liveEdit && edIsLive(t)) putUp(); }
    VIEWS[state.mode].centre(); fitAll($('canvas')); return;
  }
  if (same) { if (el.querySelector('.tsel')) render(); return; }
  fromField = pointerHeld && !!pointerEl && !el.contains(pointerEl);
  try { mutate(() => foldTyped(el, edTarget(), edEff(edTarget()))); } finally { fromField = false; }
});

/* ───────── picking, moving and resizing on the slide ───────── */
let drag = null;
document.addEventListener('pointerdown', e => {
  if ((state.mode !== 'edit' && state.mode !== 'themes') || state.reflow) return;
  const cv = e.target.closest && e.target.closest('#canvas'); if (!cv || e.target.isContentEditable) return;
  if (state.tsel && !typingEl()) { state.tsel = null; render(); }      // the words picked for the inspector are let go
  const hd = e.target.closest('[data-hd]'), ob = e.target.closest('[data-ob]');
  if (!ob) { if (state.sels.length) { state.sels = []; render(); } return; }
  const id = ob.dataset.ob, was = state.sels.length === 1 && state.sels[0] === id;
  if (e.shiftKey) { state.sels.includes(id) ? state.sels.splice(state.sels.indexOf(id), 1) : state.sels.push(id); render(); return; }
  if (!state.sels.includes(id)) { state.sels = [id]; const o0 = byId(edTarget().holder.objects, id); if (!['Box', 'Animation'].includes(state.itab)) state.itab = OWN_TAB[o0.kind] || 'Box'; render(); }
  const t = edTarget(), eff = edEff(t), moving = edPicked(t).filter(o => !o.locked); if (!moving.length) return;
  e.preventDefault();
  drag = { ids: moving.map(o => o.id), start: Object.fromEntries(moving.map(o => [o.id, { ...eff.find(x => x.id === o.id).frame }])), hd: hd ? hd.dataset.hd : '', x: e.clientX, y: e.clientY, k: 1920 / $('canvas').getBoundingClientRect().width, moved: false, was, id, before: JSON.stringify(DOC), now: {},
    linesX: [0, 960, 1920, ...eff.filter(x => !state.sels.includes(x.id) && !x.hidden).flatMap(x => [x.frame.x, x.frame.x + x.frame.w / 2, x.frame.x + x.frame.w])], linesY: [0, 540, 1080, ...eff.filter(x => !state.sels.includes(x.id) && !x.hidden).flatMap(x => [x.frame.y, x.frame.y + x.frame.h / 2, x.frame.y + x.frame.h])] };
});
document.addEventListener('pointermove', e => {
  if (!drag) return;
  let dx = (e.clientX - drag.x) * drag.k, dy = (e.clientY - drag.y) * drag.k;
  if (!drag.moved && Math.abs(dx) + Math.abs(dy) < 4 * drag.k) return;
  drag.moved = true;
  const snap = v => state.gridOn ? Math.round(v / 40) * 40 : Math.round(v);
  let gv = false, gh = false;
  for (const id of drag.ids) {
    let { x, y, w, h } = drag.start[id];
    if (!drag.hd) { x += dx; y += dy;
      if (drag.ids.length === 1 && !state.gridOn) {        // snap an edge or the middle to the slide's edges and middle, and to the other objects'
        const near = (edges, lines) => { let best = null; for (const [off] of edges) for (const l of lines) { const d = l - off; if (Math.abs(d) < 14 && (!best || Math.abs(d) < Math.abs(best.d))) best = { d, l }; } return best; };
        const sx = near([[x], [x + w / 2], [x + w]], drag.linesX), sy = near([[y], [y + h / 2], [y + h]], drag.linesY);
        if (sx) { x += sx.d; gv = sx.l; } if (sy) { y += sy.d; gh = sy.l; } } }
    else { if (drag.hd.includes('e')) w = Math.max(40, w + dx); if (drag.hd.includes('s')) h = Math.max(40, h + dy);
      if (drag.hd.includes('w')) { const nw = Math.max(40, w - dx); x += w - nw; w = nw; } if (drag.hd.includes('n')) { const nh = Math.max(40, h - dy); y += h - nh; h = nh; } }
    const f = { x: snap(x), y: snap(y), w: snap(w), h: snap(h) }; drag.now[id] = f;
    const el = document.querySelector(`#canvas [data-ob="${id}"]`);
    if (el) Object.assign(el.style, { left: f.x / 19.2 + '%', top: f.y / 10.8 + '%', width: f.w / 19.2 + '%', height: f.h / 10.8 + '%' });
  }
  const GV = document.querySelector('.guide.v'), GH = document.querySelector('.guide.h');
  GV.classList.toggle('show', gv !== false); GH.classList.toggle('show', gh !== false); if (gv !== false) GV.style.left = gv / 19.2 + '%'; if (gh !== false) GH.style.top = gh / 10.8 + '%';
});
document.addEventListener('pointerup', () => {
  if (!drag) return;
  const d = drag; drag = null;
  if (d.moved) { undoStack.push(d.before); redoStack = []; const obs = edTarget().holder.objects; for (const id of d.ids) if (d.now[id]) byId(obs, id).frame = d.now[id]; return afterChange(); }
  const o = byId(edTarget().holder.objects, d.id);
  if (d.was && o && o.kind === 'text' && !o.linked && !d.hd) startTyping(d.id);     // a second click on a picked text box types in it
});

// Adding a version to a theme: a name (one the other themes use, so the screens switch together, or a new one) and a start:
// copies of the theme's usual song templates, Bible template, or both, to change on the canvas.
A.versionNew = () => { const th = edTarget().theme, have = DOC.versions.filter(v => th.templates.some(tp => tp.version === v.id)), others = DOC.versions.filter(v => !have.includes(v));
  const songs = th.templates.filter(tp => !tp.version && (tp.for === 'songs' || tp.for === 'both')), bible = th.templates.filter(tp => !tp.version && tp.for === 'bible');
  openSheet({ title: `Add a version to “${th.name}”`, body: html`<p class="ui-p">A version is another set of templates for a moment in the service, such as the offering, communion or a kids’ segment. Switching to it by name changes every screen whose theme has a version of that name, each in its own way; the others stay as they are.</p>
    <label class="ui-f"><span>Name</span><input class="field" id="verName" list="verNames" placeholder="${others[0] ? others[0].name : 'Offering'}" value="${others[0] ? others[0].name : ''}"><datalist id="verNames">${others.map(v => html`<option value="${v.name}"></option>`)}</datalist></label>
    ${others.length ? html`<p class="ui-p sub">${others.map(v => `“${v.name}”`).join(', ')} ${others.length === 1 ? 'is' : 'are'} in another theme: use the same name so the screens switch together.</p>` : ''}
    <div class="ui-f"><span>Start from copies of</span><label class="ui-chk"><input type="checkbox" id="verSongs" ${{ checked: !!songs.length, disabled: !songs.length }}> The song templates${songs.length ? ` (${songs.map(x => x.name).join(', ')})` : ''}</label><label class="ui-chk"><input type="checkbox" id="verBible" ${{ disabled: !bible.length }}> The Bible template${bible.length ? ` (${bible[0].name})` : ''}</label></div>
    <p class="ui-p sub">A kind it has no template for (songs or Bible) keeps the usual look while the version is on.</p>`,
    buttons: [{ t: 'Cancel' }, { t: 'Add version', primary: true, f: () => { const name = $('verName').value.trim(); if (!name) { refuse('A version needs a name'); return false; }
      const pick = [...($('verSongs').checked ? songs : []), ...($('verBible').checked ? bible : [])]; if (!pick.length) { refuse('Copy at least one template to start from'); return false; }
      let made;
      mutate(() => { let v = DOC.versions.find(x => x.name.toLowerCase() === name.toLowerCase()); if (!v) { v = { id: newId('ver'), name }; DOC.versions.push(v); }
        if (th.templates.some(tp => tp.version === v.id)) return refuse(`“${th.name}” already has the ${v.name} version`);
        const copies = pick.map(tp => { const c = clone(tp); c.id = newId('tpl'); c.version = v.id; c.objects.forEach(o => { o.id = newId('o'); }); return c; }); th.templates.push(...copies); made = copies[0]; state.themeSel.tpl = made.id; state.themeSel.ver = v.id; });
 } }] }); };
M.themeVersion = ([vid]) => { const th = edTarget().theme, v = byId(DOC.versions, vid), tps = th.templates.filter(tp => tp.version === vid);
  const copyable = th.templates.filter(tp => !tp.version && !tps.some(x => x.for === tp.for && x.name === tp.name));
  return [{ head: `The ${v.name} version` },
    { t: 'Rename…', f: () => ask('Rename version', 'Name', v.name, nv => mutate(() => { v.name = nv; }), { note: 'The name is shared: every theme’s version of this name is renamed with it.' }) },
    { t: 'Add a Copy Of', sub: copyable.length ? copyable.map(tp => ({ t: `${tp.name} (${tp.for})`, f: () => mutate(() => { const c = clone(tp); c.id = newId('tpl'); c.version = vid; c.objects.forEach(o => { o.id = newId('o'); }); th.templates.push(c); state.themeSel.tpl = c.id; }) })) : [{ t: 'Every usual template is in it already', dim: true, f: () => {} }] },
    { sep: true }, { t: `Remove It From “${th.name}”`, f: () => undoable(`Remove the ${v.name} version from “${th.name}”?`, `Its ${tps.length} template${tps.length === 1 ? '' : 's'} go. Screens using this theme keep the usual look while ${v.name} is on.`, 'Remove', () => mutate(() => {
      th.templates = th.templates.filter(tp => tp.version !== vid); state.themeSel.ver = null; if (!themesWith(vid).length) { DOC.versions = DOC.versions.filter(x => x.id !== vid); if (state.version && state.version.id === vid) state.version = null; DOC.playlists.forEach(p => p.items.forEach(it => { if (it.version === vid) { delete it.version; delete it.versionKeep; } })); } state.themeSel.tpl = null; })) }]; };

// The action rows of Edit's left column, each acting on the picture picked
const moveIn = (list, item, d, all = list) => { const i = list.indexOf(item), o = list[i + d]; if (!o) return; const a = all.indexOf(item), b = all.indexOf(o); [all[a], all[b]] = [all[b], all[a]]; };
A.edTplOp = ([what]) => { const t = edTarget(), th = t.theme, ver = state.themeSel.ver || null, set = th.templates.filter(tp => (tp.version || null) === ver), tp = t.tpl;
  // a new template starts with the open one's main box and is used By hand, so it never takes over song slides by itself (as the app has it)
  if (what === 'add') return ask('New template', 'Name', '', v => mutate(() => { const main = tp && tp.objects.find(o => o.role === 'main'), n = { id: newId('tpl'), name: v, for: 'none', objects: [main ? { ...clone(main), id: newId('o') } : box('Main', 'main', { x: 120, y: 240, w: 1680, h: 600 }, { size: 96 }, 'The words of the song')] }; if (ver) n.version = ver; th.templates.push(n); state.themeSel.tpl = n.id; }), { placeholder: 'Title slide' });
  if (what === 'dup') return mutate(() => { const c = clone(tp); c.id = newId('tpl'); c.name += ' copy'; c.objects.forEach(o => { o.id = newId('o'); }); th.templates.splice(th.templates.indexOf(tp) + 1, 0, c); state.themeSel.tpl = c.id; });
  if (what === 'del') { if (!ver && set.length === 1) return refuse('A theme keeps at least one usual template');
    return undoable(`Delete “${tp.name}”?`, ver && set.length === 1 ? `It is the last template of the ${versionName(ver)} version here, so the version leaves this theme.` : 'Slides that used it take another of the theme’s templates.', 'Delete', () => mutate(() => {
      th.templates.splice(th.templates.indexOf(tp), 1); state.themeSel.tpl = null; if (ver && !th.templates.some(x => x.version === ver)) { state.themeSel.ver = null; if (!themesWith(ver).length) DOC.versions = DOC.versions.filter(x => x.id !== ver); } })); }
  mutate(() => moveIn(set, tp, what === 'up' ? -1 : 1, th.templates)); };
A.edPropOp = ([what]) => { const p = edTarget().prop, L = DOC.props;
  if (what === 'add') return A.propNew();
  if (!p) return;
  if (what === 'dup') return mutate(() => { const c = clone(p); c.id = newId('prop'); c.name += ' copy'; c.objects.forEach(o => { o.id = newId('o'); }); L.splice(L.indexOf(p) + 1, 0, c); state.edit.target = { type: 'prop', id: c.id }; });
  if (what === 'del') return undoable(`Delete “${p.name}”?`, 'It comes off the screens if it is showing.', 'Delete', () => mutate(() => { L.splice(L.indexOf(p), 1); state.propsOn = state.propsOn.filter(x => x !== p.id); state.edit.target = { type: 'prop', id: L.length ? L[0].id : null }; }));      // the last one gone: the empty Props list stays
  mutate(() => moveIn(L, p, what === 'up' ? -1 : 1)); };
A.edLayoutOp = ([what]) => { const l = edTarget().layout, L = DOC.stageLayouts;
  if (what === 'add') return A.layoutNew();
  if (what === 'dup') return mutate(() => { const c = clone(l); c.id = newId('lay'); c.name += ' copy'; c.objects.forEach(o => { o.id = newId('o'); }); L.splice(L.indexOf(l) + 1, 0, c); state.edit.target = { type: 'layout', id: c.id }; });
  if (what === 'del') { if (L.length === 1) return refuse('Keep at least one stage layout'); return undoable(`Delete “${l.name}”?`, 'Stage screens using it take the first layout.', 'Delete', () => mutate(() => { L.splice(L.indexOf(l), 1); DOC.screens.forEach(x => { if (x.layout === l.id) x.layout = L[0].id; }); state.edit.target = { type: 'layout', id: L[0].id }; })); }
  mutate(() => moveIn(L, l, what === 'up' ? -1 : 1)); };

/* ═════════ logos.js ═════════ */
// Pictures for Settings: the list's small icons, a flag for each language of the app's own words, and the logos of the
// services and assistants. A logo is here only when it is the owner's own (VoxLive, TimelyChurch) or its company's terms
// allow it and are recorded in handoff/logos.md; without one, a card shows the service's initials.
'use strict';
const ST_ICON = { Screens: svg('M1.500 2.500h10v6.500h-10zM4.500 11.500h4'), 'Phone Screens': svg('M4 1.500h5v10H4zM6 9.800h1'), Sound: svg('M2 5v3h2l3 2.500V2.500L4 5zM9.200 4.500a3 3 0 0 1 0 4'),
  Slides: svg('M1.500 3h10v7h-10zM4 6h5M5 8h3'), Groups: svg('M3 6.500h.010M6.500 6.500h.010M10 6.500h.010', 2.4), Language: svg('M6.500 1.500a5 5 0 1 0 0 10a5 5 0 0 0 0-10M1.500 6.500h10M6.500 1.500c-2 2-2 8 0 10c2-2 2-8 0-10', 1.2),
  Integrations: svg('M5 8l3-3M4.200 6.300L3 7.500a2 2 0 0 0 2.800 2.800L7 9.100M6.200 4l1.200-1.200a2 2 0 0 1 2.800 2.800L9 6.800'), 'AI assistants': svg('M6.500 1.500l1.200 3.800 3.800 1.200-3.800 1.200-1.200 3.800-1.200-3.800-3.800-1.200 3.800-1.200z', 1.2),
  'Phone remote': svg('M3.500 1.500h4v10h-4zM9.500 4.500a3 3 0 0 1 0 4'), Keys: svg('M1.500 3.500h10v6h-10zM3.500 5.500h.010M6.500 5.500h.010M9.500 5.500h.010M4.500 7.500h4', 1.2), 'Command line': svg('M2 3.500l3 3-3 3M6.500 9.500h4.500'),
  'Song files': svg('M3 1.500h4.500l2.500 2.500v7.500H3zM7.500 1.500V4H10'), About: svg('M6.500 1.500a5 5 0 1 0 0 10a5 5 0 0 0 0-10M6.500 6v3M6.500 4h.010', 1.2), Coverage: svg('M2.500 6.500l3 3 5-6') };
// A flag stands for a language here, not a country: the one most people know that language by.
const FLAGS = { en: html`<svg class="st-flag" viewBox="0 0 24 16" aria-hidden="true"><rect y="0.0" width="24" height="2.29" fill="#b22234"/><rect y="2.29" width="24" height="2.29" fill="#fff"/><rect y="4.57" width="24" height="2.29" fill="#b22234"/><rect y="6.86" width="24" height="2.29" fill="#fff"/><rect y="9.14" width="24" height="2.29" fill="#b22234"/><rect y="11.43" width="24" height="2.29" fill="#fff"/><rect y="13.71" width="24" height="2.29" fill="#b22234"/><rect width="10.5" height="9.14" fill="#3c3b6e"/></svg>`, es: html`<svg class="st-flag" viewBox="0 0 24 16" aria-hidden="true"><rect width="24" height="16" fill="#aa151b"/><rect y="4" width="24" height="8" fill="#f1bf00"/></svg>`, ru: html`<svg class="st-flag" viewBox="0 0 24 16" aria-hidden="true"><rect width="24" height="16" fill="#fff"/><rect y="5.33" width="24" height="5.34" fill="#0039a6"/><rect y="10.67" width="24" height="5.33" fill="#d52b1e"/></svg>`, uk: html`<svg class="st-flag" viewBox="0 0 24 16" aria-hidden="true"><rect width="24" height="8" fill="#0057b7"/><rect y="8" width="24" height="8" fill="#ffd700"/></svg>`,
  auto: html`<svg class="st-flag plain" viewBox="0 0 24 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="1.500" width="16" height="10" rx="1.500"/><path d="M9 14.500h6"/></svg>` };
const LOGOS = { vox: html`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none"><g transform="translate(15 24)" stroke="#F3F0E8" stroke-linecap="round" fill="none"><path d="M0 -8 Q6 0 0 8" stroke-width="2.2"/><path d="M6 -11 Q14 0 6 11" stroke-width="2"/><path d="M13 -14 Q23 0 13 14" stroke-width="1.8" opacity="0.55"/></g><circle cx="13" cy="24" r="2.4" fill="#D4A24A"/></svg>`, timely: html`<img alt="" src="logos/timelychurch.png">`,
  grok: html`<img alt="" src="logos/grok.svg">`, canva: html`<img alt="" src="logos/canva.svg">` };      // each as its company provides it, unaltered (handoff/logos.md)

/* ═════════ settings.js ═════════ */
// Settings: a page of its own behind the gear. The on-screen column stays.
'use strict';

VIEWS.settings = { left: renderSettingsLeft, centre: renderSettings };
// Regrouped by what the operator came to do (the owner, 25 November: "when i open the general tab it says, this is too much";
// "yes lets regroup"). The room: what shows and sounds. On screen: how slides look and behave. Connect: other services,
// assistants and phones. Advanced: the rest, out of everyone else's way. General and Integrations' long lists are gone.
const GROUPS = [['The room', ['Screens', 'Phone Screens', 'Sound']], ['On screen', ['Slides', 'Groups', 'Language']], ['Connect', ['Integrations', 'AI assistants', 'Phone remote']], ['Advanced', ['Keys', 'Command line', 'Song files', 'About', 'Coverage']]];
const SECTIONS = GROUPS.flatMap(g => g[1]), SEC_WAS = { General: 'Slides', 'Web control': 'Phone remote' };
const UI_LANG_OWN = { en: 'English', es: 'Español', ru: 'Русский', uk: 'Українська' };
// What a row says at its end, so a problem shows before anything is opened: [words, lit]
const secState = x => { const st = DOC.settings;
  if (x === 'Screens') { const n = DOC.screens.filter(goingOut).length; return [`${n} of ${DOC.screens.length} on`, n > 0]; }
  if (x === 'Phone Screens') return [mirrorSet().on ? 'On' : 'Off', mirrorSet().on];
  if (x === 'Slides') return [st.transition.kind, false];
  if (x === 'Language') return [UI_LANG_OWN[st.uiLang] || 'Auto', false];
  if (x === 'Integrations') { const n = INTEGRATIONS.filter(i => integState(i).tone === 'on').length; return [`${n} connected`, n > 0]; }
  if (x === 'AI assistants') return [{ off: 'Off', prep: 'Can prepare', run: 'Runs screens' }[mcpLevel()], mcpLevel() !== 'off'];
  if (x === 'Phone remote') return [st.remote ? `${PEOPLE.length} people` : 'Off', !!st.remote];
  return null; };
const PEOPLE = [{ who: 'This browser', can: 'Control', me: true }, { who: 'iPad in the front row', can: 'Watch' }];
const KEYS = [['Space  →', 'Next slide'], ['←', 'Previous slide'], ['↑  ↓', 'The item above or below in the playlist; in Bible, the verse before or after'], ['Home  End', 'First or last slide of the song'], ['Esc', 'Clear all'], ['F2 F3 F4 F5 F6', 'Clear the slide, media, props, audio, message'],
  ['A letter', 'The slide with that hot key, or the group it stands for'], ['/', 'Find a song, a line or a verse'], ['⌘B', 'The Bible page, ready for a reference'], ['⌘Z  ⇧⌘Z', 'Undo and redo'], ['⌘C ⌘X ⌘V', 'Copy, cut and paste slides, or objects in Edit'],
  ['Delete', 'Delete the picked slides, or the picked object in Edit'], ['Arrows', 'Nudge the picked object, in Edit (Shift for one pixel)'], ['T', 'A new text box, in Edit']];

function renderSettingsLeft() {
  put('left', html`<div class="sect"><div class="sect-h"><span class="cap">Settings</span></div>
    ${GROUPS.map(([g, xs]) => html`<div class="st-navg"><span class="cap">${g}</span>${xs.map(x => { const sa = secState(x);
      return html`<button class="rowbtn ${x === state.section ? 'on' : ''}" data-a="section:${x}"><span class="st-ni" aria-hidden="true">${ST_ICON[x] || ''}</span><span class="t">${x}</span>${sa ? html`${sa[1] ? html`<i class="led lit"></i>` : ''}<span class="st-rd mono">${sa[0]}</span>` : ''}</button>`; })}</div>`)}
    <button class="btn sh-add" data-a="setup" style="margin-top:14px">Set up SoftPresenter…</button></div>`);
}
A.section = ([s]) => { state.section = SEC_WAS[s] || s; state.integ = null; render(); };
// why: a reason the row does nothing now; the row is greyed and the reason takes the note's place
const stLine = (label, ctl, note = '', why = '') => html`<div class="${why ? 'sline dis' : 'sline'}"><div><b>${label}</b>${why || note ? html`<span>${why || note}</span>` : ''}</div><div class="ctl">${ctl}</div></div>`;
// A choice among a few, shown: a flat key with its light (lit when chosen), a small picture, a name and a line
const pick = (on, a, pic, name, sub = '') => html`<button class="st-pick ${on ? 'on' : ''}" role="radio" aria-checked="${on ? 'true' : 'false'}" data-a="${a}"><i class="led"></i>${pic}<span class="n">${name}</span>${sub ? html`<span class="s">${sub}</span>` : ''}</button>`;
A.setPick = ([k, v]) => mutate(() => { DOC.settings[k] = v; });
A.transPick = ([k]) => mutate(() => { const t = DOC.settings.transition; DOC.settings.transition = { ...t, kind: k, time: t.time || .5 }; });
const onoff = (on, a) => html`<button class="btn ${on ? 'onkey' : ''}" data-a="${a}">${sw(on, 'On', 'Off')}</button>`;

function renderSettings() {
  const sec = state.section, st = DOC.settings;
  $('media').hidden = true; $('foot').hidden = true; $('slides').className = 'settings';
  let body = '';
  const card = (title, sub, inner, extra = '') => html`<section class="st-card"><div class="st-ch"><b>${title}</b><span class="sub">${sub}</span><span class="sp"></span>${extra}</div>${inner}</section>`;
  if (sec === 'Screens') {
    // One Screens page (the owner, 21 November), as cards by purpose (his pick, 22 November, of three looks put side by
    // side). Each screen is a card: its live picture, its colour, a status that says why, one switch. A card opens its setup
    // as three questions (what it is for, where it goes, how the words look); the rest is under Fine-tune.
    const outN = DOC.screens.filter(goingOut).length, bad = DOC.screens.filter(x => scrState(x).tone === 'warn').length;
    const top = html`<p class="sc-sum">${outN} of ${DOC.screens.length} going out${bad ? html` · <b>${bad} need${bad === 1 ? 's' : ''} attention</b>` : ''}</p>
      <div class="sc-check"><button class="btn ${state.identify ? 'onkey' : ''}" data-a="identify" title="Each screen shows its name and colour. The room and the stream see it.">${sw(state.identify, 'Stop identifying', 'Identify')}</button><i class="rule"></i>
        <span class="cap">Test pattern</span>${seg([['off', 'Off'], ['bars', 'Colour bars'], ['grid', 'Grid'], ['white', 'White']], state.test, 'test')}<span class="sub">Everyone watching sees these</span></div>`;
    body = scrCardsHTML(top);      // what plays at start is a macro now: Macros › “Run when SoftPresenter opens” (the owner, 22 November)
  }
  if (sec === 'Slides') { const eg = DOC.songs.find(s => copyrightLine(s) && s.info && (s.info.author || s.info.ccli)) || DOC.songs[0], at = st.copyrightAt, tr = st.transition;
    // What the room sees with every song, shown: the copyright line drawn on a slide, the transition as keys with a picture
    const strip = k => html`<span class="st-strip" aria-hidden="true">${[0, 1, 2].map(i => html`<i class="${k === 'every' || (k === 'first' && i === 0) || (k === 'last' && i === 2) ? 'has' : ''}"></i>`)}</span>`;
    const three = TRANSITIONS.slice(0, 3), other = !three.includes(tr.kind);
    body = [card('Copyright line', 'Where the small line under a song’s words shows', html`<div class="st-show">
        <div class="st-slide" role="img" aria-label="A slide, with the copyright line where it shows"><div class="w">${String(slideWords(eg.slides[0]) || eg.title).split('\n').slice(0, 2).map(l => html`<span>${l}</span>`)}</div>${at === 'none' ? '' : html`<div class="c">${copyrightLine(eg) || 'Fill in a song’s details to see its line'}</div>`}</div>
        <div class="st-side"><div class="st-picks c4" role="radiogroup" aria-label="Where the copyright line shows">${[['first', 'First slide'], ['last', 'Last slide'], ['every', 'Every slide'], ['none', 'Nowhere']].map(([k, n]) => pick(k === at, `setPick:copyrightAt,${k}`, strip(k), n))}</div>
          <label class="st-ccli"><span>Your church’s CCLI licence</span><input class="field mono" data-c="setVal:ccliLicence" value="${st.ccliLicence ?? ''}"></label></div></div>`),
    card('Between slides', 'How one slide gives way to the next', html`
      <div class="st-picks c4" role="radiogroup" aria-label="Transition for all slides">${three.map(k => pick(k === tr.kind, `transPick:${k}`, html`<span class="st-tr ${k.toLowerCase()}" aria-hidden="true"><i></i><i></i></span>`, k, k === 'Cut' ? 'At once' : k === tr.kind ? tr.time + ' s' : ''))}
        ${pick(other, 'transSheet', html`<span class="st-tr more" aria-hidden="true">${I.dots || '⋯'}</span>`, other ? tr.kind : 'More…', other ? tr.time + ' s' : 'Every kind, its time and direction')}</div>
      ${stLine('Carry on to the next item', onoff(st.continuous, 'continuous'), 'Next at the end of a song opens the one after it')}`, html`<span class="sub">A song or a slide can have its own</span>`),
    card('In your window', 'How a song’s slides are laid out for you. The room never sees this', html`
      ${stLine('Slides are shown as', seg([['grid', 'Grid'], ['easy', 'Words only'], ['table', 'Table']], st.view, 'view'))}`)]; }
  if (sec === 'Language') { const cur = st.uiLang || 'auto', auto = UI_LANG_OWN[(navigator.language || 'en').slice(0, 2)] || 'English';
    // The app's own words (designed 23 November). Each language is named in itself; the remote page follows this choice.
    body = card('Language', 'The words of SoftPresenter itself: buttons, menus and messages. Changes at once', html`
      <div class="st-picks c5" role="radiogroup" aria-label="Language">${pick(cur === 'auto', 'setPick:uiLang,auto', FLAGS.auto, 'This computer’s', auto)}${Object.entries(UI_LANG_OWN).map(([k, n]) => pick(cur === k, `setPick:uiLang,${k}`, FLAGS[k], n))}</div>
      <p class="sc-note">Songs and Bibles keep their own languages: Set up SoftPresenter › Languages. The Bible page has its own reading language (Аа).</p>`); }
  if (sec === 'Song files') body = card('Song files', 'Songs are plain files that can be changed outside the app', html`
      ${stLine('Read them again', html`<button class="btn" data-a="say:Every changed song and theme was read again">Reload songs</button>`, 'Brings in songs and themes changed outside the app')}
      ${stLine('When a song was changed elsewhere', html`<button class="btn" data-a="outsideDemo">Show what happens</button>`, 'A bar offers Reload or Keep mine; nothing is swapped unasked')}`);
  if (sec === 'Groups') { const used = name => DOC.songs.reduce((n, s) => n + s.slides.filter(sl => sl.group === name).length, 0);
    body = card('Groups', 'A song’s parts: their names, colours and jump keys', html`<div class="st-groups"><span class="cap"></span><span class="cap">Name</span><span class="cap">Key</span><span class="cap">Used on</span><span></span>
      ${DOC.groups.map((g, n) => html`<button class="swt" style="background:${g.color}" data-a="menu:groupColour,${n}" title="Colour"></button>
        <input class="st-in" data-c="groupName:${n}" value="${g.name}" aria-label="Name">
        <button class="st-key mono" data-a="groupEdit:${n},key" title="Change its key">${g.key || '—'}</button>
        <span class="sub">${used(g.name) ? used(g.name) + ' slide' + (used(g.name) === 1 ? '' : 's') : 'not used'}</span>
        <button class="iconbtn" data-a="groupDel:${n}" title="Delete this group" aria-label="Delete">${I.x}</button>`)}</div>`,
      html`<button class="btn" data-a="groupAdd">${I.plus} Add a group</button>`); }
  if (sec === 'Keys') body = [card('On the buttons', 'Key tags', stLine('Show keys on buttons', onoffKeys(), 'A small tag beside each button with its key')), card('Running the show', 'These can be changed', Object.entries(KEY_NAMES).map(([a, name]) => stLine(name, html`${st.keys[a].length ? st.keys[a].map(k => html`<span class="st-key mono">${keyLabel(k)}</span>`) : html`<span class="sub">Button only</span>`}<button class="btn" data-a="keyChange:${a}">Change</button>`)),
      html`<button class="btn" data-a="keysReset">Reset</button>`),
    card('Always the same', 'Fixed keys · press ? anywhere for the whole list', KEYS.filter((_, i) => ![0, 1, 4, 5, 7].includes(i)).map(([k, what]) => stLine(what, k.split(/\s{2,}| (?=[⌘⇧F])/).map(x => html`<span class="st-key mono">${keyText(x)}</span>`))),
      html`<button class="btn" data-a="keysSheet">Every key</button>`),
    card('Groups', 'A group’s key jumps to it; change them in Groups', html`<div class="st-keygrid">${DOC.groups.filter(g => g.key).map(g => html`<span><span class="st-key mono">${g.key}</span>${g.name}</span>`)}</div>`)];
  if (sec === 'Command line') {
    // The command line: the sp command, for scripts and AI agents. As built (24 November, 7557fd8) the card shows sp's
    // state and the result of a press inline, with no notice; the app installs sp by itself at its first start from an
    // installed place (?demo=spready shows that notice). States: ?sp=missing, here, notfound, elsewhere, gone.
    const sp = cliState(), path = WINDOWS ? '%LOCALAPPDATA%\\SoftPresenter\\bin\\sp.cmd' : '~/.local/bin/sp';
    const spLine = { missing: 'sp isn’t installed yet. Install puts it where a terminal finds it.',
      here: WINDOWS ? `sp is installed (${path}). In a Command Prompt, type: sp status (in PowerShell: softpresenter status)` : `sp is installed (${path}). In a terminal, type: sp status`,
      notfound: `sp is installed (${path}), but a terminal doesn’t look in its folder yet. Install sets that up.`,
      elsewhere: 'sp opens another copy of SoftPresenter (/Applications/SoftPresenter 2.app). Use This Copy points it here.',
      gone: 'sp opens a copy of SoftPresenter that has moved or been deleted. Use This Copy points it here.' }[sp];
    const spBtns = html`${sp === 'missing' || sp === 'notfound' ? html`<button class="btn" data-a="cliDo:install" title="Puts sp (and softpresenter) where a terminal finds it">Install</button>` : ''}${sp === 'elsewhere' || sp === 'gone' ? html`<button class="btn" data-a="cliDo:use">Use This Copy</button>` : ''}${sp !== 'missing' ? html`<button class="btn ${sp === 'here' ? 'quiet' : ''}" data-a="cliDo:remove" title="Takes sp and softpresenter away again, with anything SoftPresenter added for them">Remove</button>` : ''}`;
    const cli = card('Command line', 'The sp command controls SoftPresenter from a terminal or a script.', html`
      ${stLine('The sp command', spBtns, spLine)}${state.cliSaid ? html`<p class="sc-note">${state.cliSaid}</p>` : ''}`);
    body = cli; }
  if (sec === 'Integrations') { const v = DOC.voxlive, it = INTEGRATIONS.find(x => x.id === state.integ);
    // Other services, each the same card (the owner, 25 November: "the integration cards should be the same and that goes
    // for voxlive"): its logo, name, one line, a light with its state, one key. A card opens that service's own page, in
    // the same order for all: logo and state, what it brings, the account, then More options. A new service is a new card.
    if (!it) body = html`<p class="sc-sum">Other services SoftPresenter works with. Every one opens the same way.</p><div class="st-integ">${INTEGRATIONS.map(x => { const sa = integState(x);
        return html`<button class="st-icard ${x.soon ? 'soon' : ''}" data-a="integ:${x.id}" ${{ disabled: !!x.soon }}><span class="itop">${integLogo(x)}<span class="tx"><b>${x.name}</b><span>${x.what}</span></span></span>
          <span class="ifoot"><i class="led ${sa.tone}"></i><span class="st-rd mono">${sa.text}</span><span class="btn">${x.soon ? 'Soon' : sa.tone === 'off' && x.id !== 'wla' ? 'Connect' : 'Open'}</span></span></button>`; })}
        <div class="st-icard more"><b>More to come</b><span>A new service is a new card here</span></div></div>`;
    else { const sa = integState(it);
      const head = html`<button class="st-back" data-a="integ:">${I.back || '‹'} Integrations</button>
        <div class="st-ihead">${integLogo(it, 'big')}<div class="tx"><h3>${it.name}</h3><span><i class="led ${sa.tone}"></i><span class="st-rd mono">${sa.text}</span>${it.what}</span></div></div>`;
      body = [head, it.id === 'wla' ? catCard(card) : it.id === 'vox' ? [card('VoxLive', 'Live translation captions on a screen', v.connected
        ? stLine('API key', html`<span class="st-key mono" style="text-transform:none">${v.key}…</span><button class="btn" data-a="voxDisconnect">Disconnect</button>`, 'Kept in the computer’s keychain; only its first characters are shown')
        : stLine('API key', html`<input class="field" id="voxKey" type="password" placeholder="Paste the key made in VoxLive"><button class="btn primary" data-a="voxConnect">Connect</button>`, 'Made in VoxLive › Settings › API keys, with the screens permissions'),
        html`<span class="pill" style="color:${v.connected ? 'var(--ok)' : 'var(--faint)'}">${v.connected ? 'Connected' : 'Not connected'}</span>`),
      v.connected ? card('Its screens', 'Found again every 30 seconds', [v.screens.map(sc => stLine(sc.name, html`<button class="btn" data-a="voxLook:${sc.id}">Look…</button><button class="btn ${state.vox === sc.id ? 'onkey' : ''}" data-a="vox:${sc.id}">${sw(state.vox === sc.id, 'On screen', 'Show')}</button>`, `${sc.lang ?? ''} · ${sc.mode ?? ''}`)),
        html`<p class="lede" style="margin:10px 0 12px">They are shown from the VoxLive tab on the right.</p>`]) : '']
        : card('Canva', 'The announcements video, fresh from your Canva presentation', DOC.settings.canva
          ? [stLine('The presentation', html`<button class="btn" data-a="say:Choosing the presentation is in the app">Choose…</button>`, 'Sunday announcements'), stLine('Sync', html`<button class="btn" data-a="say:Syncing from Canva">Sync Now</button><button class="btn quiet" data-a="canvaSet:">Disconnect</button>`, 'The video in the media bin is replaced when a sync finishes')]
          : stLine('Account', html`<button class="btn primary" data-a="canvaSet:1">Sign In with Canva</button>`, 'Opens Canva’s sign-in in your browser'))]; } }
  if (sec === 'AI assistants') { const lv = mcpLevel(), said = state.mcpSaid;
    // AI assistants (the app's MCP, 25 November), simplified at the owner's ask the same day: "make it simple, like a click to
    // copy, and have claude chatgpt grok and gemini and the copy will copy the how to connect mcp and explaination to the agent".
    // The two switches read as one choice of three; each assistant's key copies a message to paste into it.
    const scr = c => html`<i class="scr ${c}"></i>`, lines = html`<i class="ln"></i>`;
    body = [card('How much it may do', 'Nothing works until you choose', html`
      <div class="st-picks c3 tall" role="radiogroup" aria-label="How much an assistant may do">
        ${pick(lv === 'off', 'mcpLevel:off', html`<span class="st-lv" aria-hidden="true">${scr('')}</span>`, 'Off', 'Nothing can connect')}
        ${pick(lv === 'prep', 'mcpLevel:prep', html`<span class="st-lv" aria-hidden="true">${lines}${scr('')}</span>`, 'Prepare', 'Songs, Bible and playlists. The screens stay as they are')}
        ${pick(lv === 'run', 'mcpLevel:run', html`<span class="st-lv" aria-hidden="true">${lines}<b>→</b>${scr('lit')}</span>`, 'Prepare and run the screens', 'It asks you first, every time')}</div>
      <div class="st-cando"><div><span class="cap">With Prepare it can</span><ul>${['Find and read your songs', 'Look up and search the Bible', 'Make songs and playlists, and add to a playlist'].map(x => html`<li class="yes">${x}</li>`)}</ul></div>
        <div><span class="cap">It can never</span><ul>${['Delete anything, or open files and web pages', 'Change Settings, keys or the phone remote', 'Quit SoftPresenter'].map(x => html`<li class="no">${x}</li>`)}</ul></div></div>`),
    card('Connect one', 'Add it on this computer, or copy a message to paste in', html`
      <div class="st-ai">${AI_ASSISTANTS.map(a => { const added = !!(mcpSet().added || {})[a.id], copied = said && said.id === a.id && said.copied;
        return html`<div class="st-aicard"><span class="itop">${integLogo(a)}<span class="tx"><b>${a.name}</b><span>${added ? html`<i class="led on"></i>Added to ${a.found}` : a.found ? `${a.found} found` : `For ${a.where}`}</span></span></span>
          <span class="keys">${a.found ? html`<button class="btn ${added ? 'quiet' : ''}" data-a="mcpAdd:${a.id}" title="${added ? `Takes SoftPresenter out of ${a.found} again` : `Adds SoftPresenter to ${a.found} on this computer`}">${added ? 'Remove' : 'Add'}</button>` : ''}<button class="btn" data-a="mcpCopy:${a.id}" title="Copies a message to paste into ${a.name}: how to connect, and how to use SoftPresenter">${copied ? html`<i class="led on"></i>Copied` : 'Copy setup'}</button></span></div>`; })}</div>
      <p class="st-said ${said ? '' : 'idle'}">${said ? html`<i class="led"></i>${said.text}` : ''}<span class="sp"></span><span>ChatGPT in a browser and the Grok and Gemini apps only reach servers on the internet, so they can’t connect here.</span></p>
      <details class="st-fold"><summary>Other apps</summary>${AI_OTHER.map(a => { const added = !!(mcpSet().added || {})[a.id];
        return stLine(a.name, html`<button class="btn ${added ? 'quiet' : ''}" data-a="mcpAdd:${a.id}">${added ? 'Remove' : 'Add'}</button>`, added ? `Added to ${a.found}` : `${a.found} found`); })}</details>
      <details class="st-fold" ${{ open: !!(said && said.copied) }}><summary>What gets copied</summary><pre class="mono">${mcpSetup(said ? said.id : 'claude')}</pre></details>`)]; }
  if (sec === 'Sound') { const so = st.sound, dev = id => SOUND_OUT.find(d => d.id === id) || SOUND_OUT[0], main = dev(so.main), au = state.audio, vd = state.video;
    // the outputs this computer has: which is the main one, how many channels, whether it is there
    const outs = SOUND_OUT.map(o => html`<div class="st-out ${o.gone ? 'gone' : ''} ${o.id === so.main ? 'main' : ''}"><i class="st-led ${o.gone ? '' : 'ok'}"></i><div class="nm"><b>${o.name}</b><span>${o.gone ? 'Not connected' : `${o.ch} channels · 48 kHz`}</span></div>
        ${o.id === so.main ? html`<span class="st-tag">Main</span>` : html`<button class="btn sm" data-a="soundMain:${o.id}" ${{ disabled: !!o.gone }}>Make main</button>`}<button class="btn sm" data-a="soundTest:out,${o.id}" ${{ disabled: !!o.gone }} title="Play a short tone there">Test</button></div>`);
    // one strip per source: where it goes, which channels, how loud, muted or not
    const strip = (who, name, note, level, muted, playing) => { const chosen = dev(so[who].to), gone = so[who].to !== 'main' && chosen.gone, d = so[who].to === 'main' || gone ? main : chosen;
      return html`<div class="st-strip"><div class="src"><i class="st-led ${playing ? 'sig' : ''}" title="${playing ? 'Playing now' : 'Silent'}"></i><div><b>${name}</b><span>${note}</span></div></div>
        <div class="to"><select class="sel" data-c="sound:${who},to" aria-label="${name} goes to">${options([['main', `Main · ${main.name}`], ...SOUND_OUT.filter(o => o.id !== so.main).map(o => [o.id, o.name + (o.gone ? ' (not connected)' : '')])], so[who].to)}</select>${gone ? html`<span class="st-warn">Not connected: playing on the main output</span>` : ''}</div>
        <select class="sel" data-c="sound:${who},pair" aria-label="Channels" ${{ disabled: !(d.ch > 2) }} title="${d.ch > 2 ? 'Which pair of its channels' : 'This output has one pair of channels'}">${options(Array.from({ length: Math.max(1, d.ch / 2) }, (_, i) => [i, `${i * 2 + 1}–${i * 2 + 2}`]), Math.min(so[who].pair, d.ch / 2 - 1))}</select>
        <input type="range" class="st-range" min="0" max="100" value="${level}" data-i="${who === 'music' ? 'volume' : 'soundLevel'}" aria-label="${name} level" style="--p:${level}%"><span class="mono st-pct" id="${who === 'music' ? 'soundPctM' : 'soundPct'}">${level}%</span>
        <button class="iconbtn ${muted ? 'lit' : ''}" data-a="soundMute:${who}" title="${muted ? 'Sound back on' : 'Mute'}" aria-label="Mute">${muted ? I.mute : I.sound}</button>
        <button class="btn sm" data-a="soundTest:${who}" title="Play a short tone where this goes">Test</button></div>`; };
    body = [card('Outputs', 'What this computer can play through', html`<div class="st-outs">${outs}</div>`, html`<span class="st-fresh" title="The app plays all sound through the computer’s own output today. This is the design for choosing where it goes."><b>New</b> a design, not in the app yet</span>`),
    card('Routing', 'Where each kind of sound goes', html`<div class="st-route"><div class="st-strip head"><span>Source</span><span>Goes to</span><span>Channels</span><span>Level</span><span></span><span></span><span></span></div>
        ${strip('music', 'Music', 'Tracks in the Audio tab', au.volume, !!au.muted, au.playing)}${strip('video', 'Video sound', 'A video on the screens', so.video.level, !!vd.mute, !!videoOn() && vd.playing)}</div>`),
    card('Stream', 'What an NDI screen carries with its picture', html`
      ${stLine('Music', onoff(so.stream.music, 'soundStream:music'), 'Off keeps walk-in music out of the stream, where it may not be licensed')}
      ${stLine('Video sound', onoff(so.stream.video, 'soundStream:video'))}`)]; }
  // Phone Screens (as built, 24 November, 4f817ca…3ef5d9d; the owner: "someone airplay to softpresenter, can we build that", and
  // say it is "unofficial and maybe unstable or unsupported"): an iPhone, iPad or Mac shows its screen in a box on a slide
  if (sec === 'Phone Screens') { const m = mirrorSet();
    body = [card('Phone Screens', 'An iPhone, iPad or Mac screen in a box on a slide', html`
      ${stLine('Show an iPhone, iPad or Mac screen', onoff(m.on, 'mirrorOn'))}
      <p class="sc-note">Unofficial: works with Screen Mirroring (AirPlay) from an iPhone, iPad or Mac, but isn’t made or supported by Apple, and an Apple update may stop it working.</p>
      ${m.on ? html`
        ${stLine('The name devices see', html`<input class="field" id="mirrorName" value="${m.name}" aria-label="The name devices see"><button class="btn" data-a="mirrorName">Use</button>`)}
        <p class="sc-note">Listening on ports 7100 and 7101 (TCP), and a few UDP ports beside them; a firewall must let them in.</p>
        ${stLine('Ask me each time', onoff(m.ask, 'mirrorAskSet'), 'When a device wants to show its screen, SoftPresenter asks you in the notices before taking its picture')}
        ${m.ask ? '' : html`<p class="sc-note warn">Anyone on this network can connect without being asked. Their screen still shows only where you put a box and take the slide live.</p>`}
        ${stLine('Connected now', state.mirrorDevice ? html`<span class="sub">${state.mirrorDevice}</span><button class="btn" data-a="mirror:stop">Stop</button>` : html`<span class="sub">Nobody</span>`)}
        <p class="sc-note">On the iPhone or iPad, open Control Centre, tap Screen Mirroring and choose ${m.name}. The screen then shows in a Phone or iPad Screen box on the slide that is on screen, and nowhere else.</p>
        <p class="sc-note">The device and this computer must be on the same network. A guest Wi-Fi that keeps devices apart won’t let the device find this computer.</p>
        <p class="sc-note">Whatever is on the device’s screen is shown, messages that arrive too. There is no sound yet, and films from streaming apps may show black.</p>` : ''}`)];
  }
  // About (24 November: the Mac menu bar keeps only what a Mac user expects, so what it gave up has a home here, the one way in
  // on Windows and Linux): the version, what's new, help, reporting a problem and the licences.
  if (sec === 'About') body = [card('SoftPresenter', 'Version 0.1.0 · free and open source, GPL-3.0-or-later', html`
      ${stLine('What’s new', html`<button class="btn" data-a="say:What’s new opens: the changes in this version">What’s new</button>`, 'The changes in this version')}
      ${stLine('Help', html`<button class="btn" data-a="say:The guide opens in the browser">Open the guide</button>`, 'How to set up the screens, run a service and more, on the web')}
      ${stLine('Something wrong?', html`<button class="btn" data-a="say:A new issue opens on GitHub, with the version filled in">Report an issue…</button>`, 'Opens a new issue on GitHub, with the version filled in')}
      ${stLine('Licences', html`<button class="btn" data-a="say:The licences open">Licences…</button>`, 'SoftPresenter’s and those of the parts it carries')}`)];
  if (sec === 'Phone remote') body = [card('This page', 'Served by the computer that runs the screens, to this network only', html`
      ${stLine('Control from the network', onoff(st.remote, 'setFlag:remote'), 'Off means nobody can open this page, you included')}
      ${stLine('Address', html`<span class="st-key mono" style="text-transform:none">http://192.168.1.20:4310</span><button class="btn" data-a="say:Address copied">Copy</button>`, 'Open it on any phone, tablet or computer here')}
      ${stLine('It is showing', html`<button class="btn" data-a="menu:link">${typeof LINK !== 'undefined' && LINK.on ? 'Your SoftPresenter' : 'Sample data'} ▾</button>`, 'Sample data, or a SoftPresenter running on this computer')}`),
    card('Passwords', 'Who may watch and who may run the show', html`
      ${stLine('To watch', html`<input class="field" type="password" value="lanterns" aria-label="Password to watch">`, 'Sees the slides and what is on screen')}
      ${stLine('To control', html`<input class="field" type="password" value="harbour-lights" aria-label="Password to control">`, 'Can show slides, clear and edit')}`),
    card('Connected now', `${PEOPLE.length} ${PEOPLE.length === 1 ? 'person' : 'people'}`, PEOPLE.map((p, n) => stLine(p.who, p.me ? html`<span class="pill">You</span>` : html`<button class="btn" data-a="dropPerson:${n}">Disconnect</button>`, p.can)))];
  if (sec === 'Coverage') body = coverageHTML();
  put('slides', html`<div class="reader-h"><h1>${sec}</h1></div>${body}`);
}
A.say = args => toast(args.join(','));      // the whole sentence, commas and all
A.setFlag = ([k]) => mutate(() => { DOC.settings[k] = !DOC.settings[k]; });
C.setVal = (v, [k]) => mutate(() => { DOC.settings[k] = v; });
// Phone Screens: off until switched on; Ask me each time on unless changed; the name devices see
const mirrorSet = () => DOC.settings.mirror || (DOC.settings.mirror = { on: false, ask: true, name: 'SoftPresenter — Church Mac' });
A.mirrorOn = () => mutate(() => { const m = mirrorSet(); m.on = !m.on; if (!m.on) { state.mirrorDevice = null; state.mirrorAsk = null; } });
A.mirrorAskSet = () => mutate(() => { const m = mirrorSet(); m.ask = !m.ask; });
// The answers to a device that asks, and Stop
A.mirror = ([what]) => { const name = state.mirrorAsk || state.mirrorDevice;
  if (what === 'allow' && state.mirrorAsk) { state.mirrorAsk = null; state.mirrorDevice = name; toast(`${name} is showing its screen.`); }
  else if (what === 'deny' && state.mirrorAsk) { state.mirrorAsk = null; toast(`${name} was not let in.`); }      // and not asked about again for a minute
  else if (what === 'stop' && state.mirrorDevice) { state.mirrorDevice = null; toast(`${name} stopped showing its screen.`); }
  render(); };
A.mirrorName = () => { const v = ($('mirrorName') || {}).value || ''; if (!v.trim()) return refuse('Devices need a name to find this computer by'); mutate(() => { mirrorSet().name = v.trim(); }); };
// The menu that chooses the app's own words, on the Language card and in the setup's first step (as built, 24 November):
// the computer's language first, named in that language, then English, Español, Русский, Українська, each written in itself.
// Integrations: one entry per service; a new service is a new entry. `soon`: shown greyed until it works.
const INTEGRATIONS = [{ id: 'wla', name: 'Worship Leader App', what: 'Free songbooks, one for each language' }, { id: 'vox', name: 'VoxLive', what: 'Live translation captions on a screen' },
  { id: 'canva', name: 'Canva Sync', what: 'The announcements video, fresh from your Canva presentation' }, { id: 'timely', name: 'TimelyChurch', what: 'Service plans and set lists from your church planner', soon: true }];
const integState = x => { if (x.soon) return { tone: 'soon', text: 'Coming soon' };
  if (x.id === 'wla') { const n = Object.keys(catHave()).length; return n ? { tone: 'on', text: `${n} songbook${n === 1 ? '' : 's'}` } : { tone: 'off', text: 'None yet' }; }
  if (x.id === 'vox') return DOC.voxlive.connected ? { tone: 'on', text: `Connected · ${DOC.voxlive.screens.length} screens` } : { tone: 'off', text: 'Not connected' };
  return DOC.settings.canva ? { tone: 'on', text: 'Signed in' } : { tone: 'off', text: 'Not connected' }; };
// A service's logo: its own mark when logos.js has one (each used under that company's terms), else its initials
const integLogo = (x, size = '') => html`<span class="st-logo ${size}" aria-hidden="true">${typeof LOGOS === 'object' && LOGOS[x.id] ? LOGOS[x.id] : x.name.split(/\s+/).map(w => w[0]).join('').slice(0, 2)}</span>`;
A.integ = ([id]) => { state.integ = id || null; render(); };
A.canvaSet = ([on]) => mutate(() => { DOC.settings.canva = !!on; });
// AI assistants (the app's MCP, crates/sp-app/src/mcp): the app keeps two switches (allowed, and changing the screens, which
// goes off with the first); here they read as one choice of three.
const mcpSet = () => DOC.settings.mcp || (DOC.settings.mcp = { allow: false, screens: false });
const mcpLevel = () => { const m = mcpSet(); return !m.allow ? 'off' : m.screens ? 'run' : 'prep'; };
A.mcpLevel = ([l]) => { state.mcpSaid = null; mutate(() => { const m = mcpSet(); m.allow = l !== 'off'; m.screens = l === 'run'; }); };
// The program an assistant starts (the installed sp on a Mac and Linux, the program itself on Windows, where sp is a batch
// file), written as a shell takes it, as pane.rs does
const mcpProgram = () => WINDOWS ? 'C:\\Program Files\\SoftPresenter\\SoftPresenter.exe' : '/Users/you/.local/bin/sp';
const shellWord = p => /^[A-Za-z0-9\/._\\:-]+$/.test(p) ? p : WINDOWS ? `"${p}"` : `'${p.replace(/'/g, `'\\''`)}'`;
const mcpDesktop = p => `{\n  "mcpServers": {\n    "softpresenter": {\n      "command": ${JSON.stringify(p)},\n      "args": ["mcp"]\n    }\n  }\n}`;
// Only the versions that run on this computer can start a local program; the websites can't. `add`: that assistant's own way
// to add a local server (Claude's is the app's; the other three are to be checked against each company's own pages).
const AI_ASSISTANTS = [{ id: 'claude', name: 'Claude', where: 'the desktop app or Claude Code', found: 'Claude Desktop and Claude Code', add: w => `In Claude Code, run:  claude mcp add --scope user softpresenter -- ${w} mcp\nIn Claude Desktop, which can't run that, tell me to add this to claude_desktop_config.json and open Claude Desktop again:\n${mcpDesktop(mcpProgram())}` },
  { id: 'chatgpt', name: 'ChatGPT', where: 'the desktop app or Codex', found: 'Codex', add: w => `Run:  codex mcp add softpresenter -- ${w} mcp\nThe ChatGPT desktop app, the Codex CLI and its IDE extension share that configuration.` },
  { id: 'grok', name: 'Grok', where: 'Grok Build', found: '', add: w => `In Grok Build, run:  grok mcp add -s user softpresenter -- ${w} mcp` },
  { id: 'gemini', name: 'Gemini', where: 'Antigravity', found: 'Antigravity', add: () => `In Antigravity (which took Gemini CLI's place), add this under mcpServers in ~/.gemini/config/mcp_config.json, leaving the rest of the file as it is:\n${mcpDesktop(mcpProgram())}` }];
// Other apps the app can add itself to, under More (the app's backend, 25 November): not among the owner's four
const AI_OTHER = [{ id: 'cursor', name: 'Cursor', found: 'Cursor' }, { id: 'vscode', name: 'VS Code / GitHub Copilot', found: 'VS Code' }];
// What one click copies: how to connect, then how to use it, written to the assistant
const mcpSetup = id => { const a = AI_ASSISTANTS.find(x => x.id === id) || AI_ASSISTANTS[0], w = shellWord(mcpProgram());
  return `Connect to SoftPresenter, the church presentation app running on this computer. It has an MCP server.

To connect
${a.add(w)}
If that doesn't match your version: it is a local MCP server over stdio. The command is ${w} and its one argument is mcp.

How to use it
- Its tools find and read songs, look up and search the Bible, list and make songs and playlists, and add to a playlist. Use them to get a service ready with me.
- Changing what is on the screens (showing a song, an item or a passage, next, previous, clear, a message) works only when I have allowed it in SoftPresenter's Settings. Ask me first, every time, and say the words that will go up.
- You can't delete anything, change its settings, open files or web pages, or quit it. If a tool is refused, tell me what it said.
- Start by asking it what is on screen and which playlist is open.`; };
// Add: the app writes SoftPresenter into that company's apps on this computer (their own configuration, kept a copy of), and
// Remove takes it out again (the app session's `sp mcp connect|disconnect`)
A.mcpAdd = ([id]) => { const a = AI_ASSISTANTS.concat(AI_OTHER).find(x => x.id === id); if (!a || !a.found) return; const m = mcpSet(); m.added = m.added || {};
  const on = !m.added[id]; mutate(() => { m.added[id] = on; });
  state.mcpSaid = { id, text: on ? `Added to ${a.found}. Open ${a.found.includes(' and ') ? 'them' : 'it'} again to see SoftPresenter.${mcpLevel() === 'off' ? ' It can connect once you choose Prepare.' : ''}` : `Removed from ${a.found}.` }; render(); };
A.mcpCopy = ([id]) => { const a = AI_ASSISTANTS.find(x => x.id === id); if (!a) return;
  if (navigator.clipboard) navigator.clipboard.writeText(mcpSetup(id)).catch(() => {});
  state.mcpSaid = { id, copied: true, text: `Copied. Paste it into ${a.name}.${mcpLevel() === 'off' ? ' It can connect once you choose Prepare.' : ''}` }; render(); };
// The sp command's state for the Command line card (?sp= shows one), and what a press there says, inline
const cliState = () => state.cli || new URLSearchParams(location.search).get('sp') || 'here';
A.cliDo = ([what]) => { const shell = WINDOWS ? null : '~/.zshrc', folder = WINDOWS ? '%LOCALAPPDATA%\\SoftPresenter\\bin' : '~/.local/bin';
  if (what === 'remove') { state.cli = 'missing'; state.cliSaid = WINDOWS ? 'Removed sp and softpresenter, and took their folder off your PATH.' : `Removed sp and softpresenter, and the line SoftPresenter added to ${shell}.`; }
  else { state.cli = 'here'; state.cliSaid = what === 'use' ? null      // the state line then says where sp is
    : WINDOWS ? `Installed in ${folder}, which is on your PATH. Open a new Command Prompt and type: sp status (in PowerShell: softpresenter status)` : `Installed in ${folder}, and SoftPresenter added a line for it to ${shell}. Open a new terminal window and type: sp status`; }
  render(); };
const uiLangMenu = () => { const own = { en: 'English', es: 'Español', ru: 'Русский', uk: 'Українська' };
  const choices = [['auto', `The computer’s language (${own[(navigator.language || 'en').slice(0, 2)] || 'English'})`], ...Object.entries(own)];
  return html`<select class="sel" data-c="setVal:uiLang">${options(choices, DOC.settings.uiLang || 'auto')}</select>`; };
// Sound: the outputs this computer has (a sample), and where music and video sound are sent
const SOUND_OUT = [{ id: 'default', name: 'Built-in output', ch: 2 }, { id: 'usb', name: 'USB audio interface', ch: 4 }, { id: 'hdmi', name: 'HP 27f (HDMI)', ch: 2 }, { id: 'tv', name: '3DTV', ch: 2, gone: true }];
C.sound = (v, [who, key]) => mutate(() => { const so = DOC.settings.sound; if (who === 'main') so.main = v; else { so[who][key] = key === 'pair' ? +v : v; if (key === 'to') so[who].pair = 0; } });
A.soundStream = ([k]) => mutate(() => { const s = DOC.settings.sound.stream; s[k] = !s[k]; });
A.soundMain = ([id]) => mutate(() => { const so = DOC.settings.sound; so.main = id; for (const k of ['music', 'video']) if (so[k].to === id) { so[k].to = 'main'; so[k].pair = 0; } });
A.soundMute = ([who]) => { if (who === 'music') { state.audio.muted = !state.audio.muted; syncAudio(); } else { state.video.mute = !state.video.mute; syncVideo(); } render(); };
A.soundTest = ([who, id]) => { const so = DOC.settings.sound, pick = who === 'out' ? id : so[who].to === 'main' ? so.main : so[who].to, d = SOUND_OUT.find(o => o.id === pick), use = d.gone ? SOUND_OUT.find(o => o.id === so.main) : d, pair = who === 'out' ? 0 : Math.min(so[who].pair, use.ch / 2 - 1);
  toast(`A test tone plays on ${use.name}${use.ch > 2 ? `, channels ${pair * 2 + 1}–${pair * 2 + 2}` : ''}`); };
A.dropPerson = ([n]) => { const p = PEOPLE.splice(+n, 1)[0]; render();  };

/* screens */
// Sample hardware. DeckLink outputs carry the names given in Desktop Video Setup (an unnamed one reads like "DeckLink Duo (3)");
// outputs the card's present profile can't use aren't offered. `def` is the card's own default format, when one is set there.
// Which formats can key comes from the card, format by format: one keys when the card offers 8-bit BGRA there (alpha travels
// only in BGRA) and says it can key. The sample is the church's Duo 2 as Desktop Video 16.3 lists it (22 November).
const DECKLINK = { Lyrics: { conn: 'SDI 3+4', def: '1080i59.94' }, Bible: { conn: 'SDI 1+2', def: null } };
const isDeckLink = to => to in DECKLINK;
const FORMATS = ['1080i50', '1080i59.94', '1080p23.98', '1080p24', '1080p25', '1080p29.97', '1080p30', '1080p50', '1080p59.94', '1080p60'];
const YUV_ONLY = ['1080p50', '1080p59.94', '1080p60'], noKeying = f => YUV_ONLY.includes(f);      // offered in YUV only, so no alpha to key with
// where a DeckLink output's format came from, in the app's words (master 441b0c2)
const formatFrom = x => { const def = (DECKLINK[x.to] || {}).def; return x.formatFrom === 'card' ? `${x.format}, as Desktop Video Setup has it.` : x.formatFrom === 'chosen' ? `${x.format}, set in SoftPresenter${def ? `: Desktop Video Setup has ${def}` : ''}.` : `${x.format}, the usual HD format: Desktop Video Setup sets none.`; };
const DISPLAYS = [{ name: 'This display', main: true, size: '2560 × 1440' }, { name: 'HP 27f', size: '1920 × 1080' }, { name: 'Stage TV', size: '1920 × 1080' }, { name: '3DTV', size: '1920 × 1080', gone: true }];
// Each screen keeps a colour of its own (Identify shows it); a new screen takes the first one free. Never red: red means live.
const IDENT = [['#4285f4', 'Blue'], ['#34c56b', 'Green'], ['#f4a000', 'Orange'], ['#9b59b6', 'Purple'], ['#4fc3f7', 'Light blue'], ['#ffd54f', 'Yellow'], ['#5c6bc0', 'Indigo'], ['#c8a27a', 'Tan']];
const ALL_LAYERS = ['slide', 'media', 'props', 'messages', 'announcements'], LAYER_NAMES = { slide: 'Slides', media: 'Media', props: 'Props', messages: 'Messages', announcements: 'Announcements' };
const kindOn = x => x.kind === 'Audience' ? state.audience : state.stage;
// Placing runs down this order and the first screen that asks for a display, NDI name or DeckLink output has it: audience
// screens first, as listed, then stage screens. So the order shown is the real order.
const order = () => [...DOC.screens.filter(x => x.kind === 'Audience'), ...DOC.screens.filter(x => x.kind === 'Stage')];
const ndiOf = x => (x.ndiName || '').trim();
const sameOut = (a, b) => !!a.to && a.to === b.to && a.to !== 'A window' && (a.to !== 'NDI' || (!!ndiOf(a) && ndiOf(a) === ndiOf(b)));
const takenBy = x => { for (const y of order()) { if (y === x) return null; if (y.on && sameOut(y, x)) return y; } return null; };
const goingOut = x => !!x.to && x.on && kindOn(x) && !x.disconnected && !takenBy(x) && !(x.to === 'NDI' && !ndiOf(x));
const onNet = x => x.to === 'NDI' || isDeckLink(x.to);
const outKind = x => x.to === 'NDI' ? 'NDI' : isDeckLink(x.to) ? 'a DeckLink output' : 'a display or window';
const keySources = x => DOC.screens.filter(y => y.kind === 'Audience' && y.id !== x.id && !y.keyOf);
const dispOf = x => x.to && x.to.startsWith('Display: ') ? x.to.slice(9) : null;
// What a screen is doing, in a few words for its row (t) and in full for its settings (long), with the one fix that helps.
// tone: ok (going out), off (switched off, by choice), warn (trouble: amber, never the grey of Off). where: which heading it belongs to.
function scrState(x) {
  const aud = x.kind === 'Audience', D = dispOf(x), off = (t, long) => ({ tone: 'off', t, long }), ok = t => ({ tone: 'ok', t }), warn = (t, long, fix, where = 'out') => ({ tone: 'warn', t, long, fix, where });
  if (!x.to) return x.on ? warn('Choose where it goes', 'Choose where it goes: a display, a window, NDI or a DeckLink output.') : off('Not set up yet', 'Choose what it is for and where it goes.');
  if (!x.on) return off(onNet(x) ? 'Off · sending black' : 'Off', onNet(x) ? 'Switched off. It keeps sending black, so a stream or switcher keeps it as a source.' : 'Switched off. It keeps where it goes, ready for when it is switched on.');
  if (!kindOn(x)) return off(`All ${aud ? 'audience' : 'stage'} screens are off`, `All ${aud ? 'audience' : 'stage'} screens are switched off${onNet(x) ? '; this one sends black meanwhile' : ''}.`);
  if (x.disconnected) return warn(`${D} isn’t plugged in`, `“${D}” isn’t plugged in. This screen shows nothing until it is back, then goes back to it by itself; it never moves to another display.`, ['Use a window for now', `scrSet:${x.id},to,A window`]);
  const t = takenBy(x), what = x.to === 'NDI' ? `the NDI name “${ndiOf(x)}”` : D || x.to;
  if (t) return warn(`${t.name} has ${x.to === 'NDI' ? 'that NDI name' : what}`, `“${t.name}” already uses ${what}, and the first screen in the list has it, so this shows nothing.`, [`Take it from ${t.name}`, `scrTake:${x.id}`]);
  if (x.to === 'NDI' && !ndiOf(x)) return warn('Needs an NDI name', 'Give the NDI output a name, so receivers can find it.');
  if (aud && x.keyOf) { const f = byId(DOC.screens, x.keyOf);
    if (!f) return warn('Its fill has gone: black', 'The screen it was the key of has been removed, so it shows black.', null, 'pic');
    if (f.kind !== 'Audience') return warn(`${f.name} is a stage screen: black`, `“${f.name}” is a stage screen, which has no key, so this shows black.`, null, 'pic');
    if (!goingOut(f)) return warn(`${f.name} isn’t going out: black`, `“${f.name}” isn’t going out, so this shows black.`, null, 'pic');
    if (onNet(f) !== onNet(x)) return warn(`${f.name} is on ${outKind(f)}: black`, `“${f.name}” is on ${outKind(f)}. A key goes out beside its fill on two displays or windows, or on two NDI or DeckLink outputs, so this shows black.`, null, 'pic');
    if (f.bgMode !== 'clear') return warn(`${f.name} is solid: all white`, `“${f.name}” is solid, so its key is all white.`, [`Make ${f.name} see-through`, `scrPick:${f.id},bgMode=clear`], 'pic');
  }
  const keyer = isDeckLink(x.to) ? x.keyer || 'off' : 'off';
  if (keyer !== 'off' && noKeying(x.format)) return warn(`Can’t key at ${x.format}`, `${x.to} can’t key at ${x.format}: the card sends that format as YUV only. Choose a format under “Can key”.`, ['Use 1080i59.94', `scrPick:${x.id},format=1080i59.94`], 'pic');
  if (x.to === 'NDI') return ok(`On NDI · ${x.receivers ? `${x.receivers} watching` : 'nobody watching'}`);
  if (isDeckLink(x.to)) return ok(`Playing at ${x.format}${keyer === 'fill' ? ', fill and key' : keyer === 'over' ? ', over the card’s input' : ''}`);
  if (D) return ok(`Filling ${D}`);
  return ok('In a window');
}
const scrPic = x => html`${x.kind === 'Audience' ? screenHTML(x) : stageHTML(x.layout)}${x.disconnected && x.on ? html`<div class="off">NOT PLUGGED IN</div>` : !goingOut(x) ? html`<div class="off">OFF</div>` : ''}`;
const scrNote = (s, where, plain) => s.tone === 'warn' && s.where === where ? html`<p class="sc-note warn">${s.long}${s.fix ? html`<button class="btn sm" data-a="${s.fix[1]}">${s.fix[0]}</button>` : ''}</p>` : plain ? html`<p class="sc-note">${plain}</p>` : '';
// Fine-tune: the rest of an audience screen's settings, under three fixed headings. A row that does nothing in a state is
// greyed with the reason, not hidden, so nothing jumps.
function scrFineHTML(x) {
  const s = scrState(x), clear = x.bgMode === 'clear' && !x.keyOf, key = !!x.keyOf, f = key && byId(DOC.screens, x.keyOf);
  const sec = (title, body, extra = '', dim = '') => html`<div class="sc-sec ${dim ? 'dim' : ''}"><div class="sc-sh"><span class="cap">${title}</span>${dim ? html`<span class="sub">${dim}</span>` : ''}<span class="sp"></span>${extra}</div>${body}</div>`;
  const mode = key ? 'key' : x.bgMode === 'clear' ? 'clear' : 'solid', k = isDeckLink(x.to) ? x.keyer || 'off' : 'off';
  const why = mode === 'key' ? `Shows the key of “${f ? f.name : 'a screen that has gone'}”: white where it is solid, black where it is clear, made from the very picture it shows, so the two never differ.`
    : mode === 'clear' ? `Clear wherever nothing is drawn, so a switcher or streaming software can lay the words over a camera. The media and the themes’ backgrounds are left out; the words, props, messages and announcements stay.${onNet(x) ? (x.to === 'NDI' ? ' Over NDI the clear parts go with the picture.' : '') : ' A display can’t be clear, so here it shows them on black; another screen can be its key.'}`
    : 'As usual: the background fills the picture.';
  const noKey = !keySources(x).length && !key ? (DOC.screens.some(y => y.kind === 'Audience' && y.id !== x.id) ? 'The other audience screens are keys themselves' : 'There is no other audience screen for this one to show the key of') : '';
  const keyed = isDeckLink(x.to) && k !== 'off';      // a keyed DeckLink screen is see-through by its keyer: no Background or Soft edges here
  const pic = sec('Picture', keyed ? [scrNote(s, 'pic', why)] : [stLine('Background', [seg([['solid', 'Solid'], ['clear', 'See-through'], ['key', 'A key']].map(([v, t]) => [`${x.id},${v}`, t]), `${x.id},${mode}`, 'scrPic', { [`${x.id},key`]: noKey ? { disabled: true, title: noKey } : null }),
        key ? html`<select class="sel" data-c="scr:${x.id},keyOf" aria-label="The key of">${options([...(f ? [] : [['', 'of a screen that has gone']]), ...keySources(x).map(y => [y.id, 'of ' + y.name])], x.keyOf)}</select>` : ''], 'Solid: a full picture, as usual. See-through: clear wherever nothing is drawn, for lower thirds over a camera: a video switcher or streaming app lays the words over its camera picture.'),
      stLine('Soft edges', scrSwitch(x, 'alpha', [['straight', 'Straight'], ['premultiplied', 'Premultiplied']], 'straight'), 'How a see-through edge is sent: try the other if the words’ edges look dark', mode === 'clear' ? '' : 'Only for a see-through screen'),
      scrNote(s, 'pic', why)]);
  const allShown = x.layers.length === 5 && (x.colour || '#000000') === '#000000' && !(x.edges || []).some(Number), E = Array.isArray(x.edges) ? x.edges : [0, 0, 0, 0];
  const shows = sec('What it shows', stLine('Layers', ALL_LAYERS.map(l => html`<button class="tg ${x.layers.includes(l) ? 'act' : ''} ${l === 'media' && clear ? 'na' : ''}" data-a="scrLayer:${x.id},${l}" ${{ title: l === 'media' && clear ? 'Not drawn on a see-through screen' : null }}>${LAYER_NAMES[l]}</button>`), 'A lobby screen might show only media and announcements'),
    html`<button class="btn sm" data-a="scrAll:${x.id}" ${{ disabled: allShown || key }} title="Every layer, black, nothing covered. Keeps the theme, the background and the key.">Show everything</button>`, key ? 'Not used by a key screen' : '');
  const fit = sec('Fit to the screen', [stLine('Colour', html`<input class="sc-colour" type="color" data-c="scr:${x.id},colour" value="${x.colour || '#000000'}" aria-label="Colour">`, 'Shown where nothing is drawn: around the slide, and when the screen is clear', clear ? 'Clear on a see-through screen' : ''),
    stLine('Cover edges', html`<div class="sc-edges">${['Left', 'Top', 'Right', 'Bottom'].map((n, i) => html`<input class="num e${i}" type="number" min="0" max="540" data-c="scr:${x.id},edge${i}" value="${E[i]}" aria-label="${n}" title="${n}">`)}<div class="sc-ebox"><i style="border-width:${E[1] / 1080 * 40}px ${E[2] / 1920 * 72}px ${E[3] / 1080 * 40}px ${E[0] / 1920 * 72}px"></i></div></div>`, `In the slide’s pixels (it is 1920 × 1080), for a screen that cuts its edges off${clear ? '; cut clear on a see-through screen' : ''}`)], '', key ? 'Not used by a key screen' : '');
  return html`<div class="sc-more">${pic}${shows}${fit}</div>`;
}
// Under a screen's setup: its order, its colour, and Remove (what it is for sets audience or stage)
function scrFootHTML(x) {
  const same = DOC.screens.filter(y => y.kind === x.kind), at = same.indexOf(x);
  const only = x.kind === 'Audience' && DOC.screens.filter(y => y.kind === 'Audience').length === 1;
  return html`<div class="sc-foot"><button class="btn sm" data-a="scrMove:${x.id},-1" ${{ disabled: at === 0 }}>Move up</button><button class="btn sm" data-a="scrMove:${x.id},1" ${{ disabled: at === same.length - 1 }}>Move down</button><button class="btn sm" data-a="menu:scrColour,${x.id}"><i class="sc-chip" style="background:${x.tint}"></i>Colour ▾</button><span class="sp"></span><button class="btn sm" data-a="scrRemove:${x.id}" ${{ disabled: only, title: only ? 'There is always one audience screen' : null }}>Remove…</button></div>`;
}
// Under "Where does it go?": what that output does, or the trouble with its fix
function scrDestNote(x) {
  const s = scrState(x), D = dispOf(x);
  const help = !x.to ? 'Choose a display, a window, NDI or a DeckLink output.'
    : D ? `Fills ${D}. If it is unplugged, this screen waits for it and never moves to another display.`
    : x.to === 'A window' ? 'An ordinary window, 960 × 540, for trying things out or for capturing in other software.'
    : x.to === 'NDI' ? (ndiState() === 'none' ? '' : `Other computers on the network can show this screen, at 1920 × 1080. NDI® is a registered trademark of Vizrt NDI AB.${s.tone === 'ok' ? ` Now: ${s.t.replace('On NDI · ', '')}.` : ''}${WINDOWS ? ' The first time, Windows Defender Firewall asks whether to allow SoftPresenter: choose Allow for private networks.' : ''}`)
    : `Played out of the DeckLink card’s ${DECKLINK[x.to].conn}, in the format chosen here. A new slide fades in as it does on the other screens.`;      // the dissolve reaches NDI and DeckLink (the app, 170123b)
  return html`${scrNote(s, 'out', help)}${x.to === 'NDI' ? ndiGet() : ''}`;
}
// NDI on a computer without it (as built, 24 November). A Mac and Windows point to NDI Tools. Linux has no NDI Tools, so the
// app downloads NDI's runtime from NDI's own server, shows NDI's licence from inside the download, and installs only after
// Accept, into its own data folder; nothing is accepted for anyone.
const NDI_MB = 68;
function ndiGet() {
  const d = state.ndiDl, link = html`<button class="btn sm" data-a="say:ndi.video opens in your browser">ndi.video ↗</button>`;      // the app opens it; this page stays on its own address
  if (LINUX && state.ndiInstalled && ndiState() !== 'none') return html`<p class="sc-note">NDI 6.3.2 installed (downloaded 24 Nov 2026) <button class="btn sm" data-a="ndiDl:remove" title="Deletes the NDI that SoftPresenter downloaded">Remove</button></p>`;
  if (ndiState() !== 'none') return '';
  if (!LINUX) return html`<p class="sc-note">NDI isn’t installed on this computer. Install NDI Tools (free), then turn this screen on again: ${link}</p>`;
  if (d && d.got != null) return html`<div class="sc-note ndi-dl"><span>${d.got < NDI_MB ? `Downloading NDI… ${d.got} of ${NDI_MB} MB` : 'Checking the download…'}</span><progress max="${NDI_MB}" value="${d.got}"></progress><button class="btn sm" data-a="ndiDl:cancel">Cancel</button></div>`;
  if (d === 'licence') return html`<div class="sc-note ndi-dl"><b>NDI’s licence</b><div class="ndi-lic">NDI’s own licence, read from inside the download, scrolls here. Nothing is installed until you accept it.</div><div class="ndi-btns"><button class="btn sm" data-a="ndiDl:cancel">Cancel</button><button class="btn sm primary" data-a="ndiDl:accept">Accept</button></div></div>`;
  return html`<p class="sc-note">NDI isn’t installed on this computer. ${link} <button class="btn sm" data-a="ndiDl:start">Download NDI…</button></p>`;
}
let ndiTick = 0;
A.ndiDl = ([what]) => {
  clearInterval(ndiTick);
  if (what === 'start') { state.ndiDl = { got: 0 }; ndiTick = setInterval(() => { const d = state.ndiDl; if (!d || d.got == null) return clearInterval(ndiTick);
    d.got = Math.min(NDI_MB, d.got + 12); if (d.got >= NDI_MB) { clearInterval(ndiTick); setTimeout(() => { if (state.ndiDl && state.ndiDl.got != null) { state.ndiDl = 'licence'; render(); } }, 400); } render(); }, 250); }
  else if (what === 'cancel') state.ndiDl = null;      // the download is deleted
  else if (what === 'accept') { state.ndiDl = null; state.ndi = 'found'; state.ndiInstalled = true; toast('NDI is ready: screens sent over NDI start now.'); }
  else if (what === 'remove') { state.ndiInstalled = false; state.ndi = 'none'; toast('NDI was removed. Restart SoftPresenter to stop using it.'); }
  render();
};
const nextTint = () => (IDENT.find(([c]) => !DOC.screens.some(x => x.tint === c)) || IDENT[DOC.screens.length % IDENT.length])[0];
// A new screen starts switched off, going nowhere, and opens ready to set up: adding one never changes what anyone sees.
A.scrAdd = ([kind]) => mutate(() => { const n = DOC.screens.filter(x => x.kind === kind).length + 1, base = { id: newId('scr'), kind, to: null, on: false, purpose: '', tint: nextTint() };
  DOC.screens.push(kind === 'Audience' ? { ...base, name: `Screen ${n}`, theme: null, layers: [...ALL_LAYERS], colour: '#000000', edges: [0, 0, 0, 0], bgMode: 'solid' } : { ...base, name: `Stage screen ${n}`, layout: DOC.stageLayouts[0].id });
  state.scrOpen = base.id; });
A.scrOn = ([id]) => mutate(() => { const x = byId(DOC.screens, id); x.on = !x.on; });
A.scrTake = ([id]) => { const x = byId(DOC.screens, id), t = takenBy(x); if (!t) return;      // it blanks a screen people may be watching, so it asks, as Remove does
  const what = x.to === 'NDI' ? 'the NDI name' : dispOf(x) || x.to;
  confirmDo(`Take ${what} from ${t.name}?`, `${t.name} stops showing at once and is switched off, keeping its settings. ${x.name} goes out in its place.`, 'Take it', () => mutate(() => { t.on = false; })); };
A.scrRemove = ([id]) => { const x = byId(DOC.screens, id);
  if (x.kind === 'Audience' && DOC.screens.filter(y => y.kind === 'Audience').length === 1) return refuse('There is always one audience screen. Switch it off to show nothing.');
  confirmDo(`Remove the screen “${x.name}”?`, 'Nothing more is sent to it, and its settings go. This can’t be undone.', 'Remove', () => mutate(() => { DOC.screens.splice(DOC.screens.indexOf(x), 1); if (state.scrOpen === id) state.scrOpen = null; })); };
A.scrMove = ([id, d]) => mutate(() => { const L = DOC.screens, x = byId(L, id), same = L.filter(y => y.kind === x.kind), y = same[same.indexOf(x) + +d]; if (!y) return; const a = L.indexOf(x), b = L.indexOf(y); [L[a], L[b]] = [L[b], L[a]]; });
A.scrPic = ([id, mode]) => { const x = byId(DOC.screens, id);
  if (mode === 'key') { const f = keySources(x)[0]; if (!f) return refuse('There is no other audience screen for this one to be the key of'); return mutate(() => { x.keyOf = x.keyOf || f.id; }); }
  mutate(() => { x.keyOf = ''; x.bgMode = mode; }); };
A.scrAll = ([id]) => mutate(() => { const x = byId(DOC.screens, id); x.layers = [...ALL_LAYERS]; x.colour = '#000000'; x.edges = [0, 0, 0, 0]; });
M.scrRow = ([id]) => { const x = byId(DOC.screens, id), same = DOC.screens.filter(y => y.kind === x.kind), i = same.indexOf(x);
  return [{ head: x.name }, { t: 'Move up', dim: i === 0, f: () => i > 0 && A.scrMove([id, -1]) }, { t: 'Move down', dim: i === same.length - 1, f: () => i < same.length - 1 && A.scrMove([id, 1]) },
    { t: 'Colour', sub: M.scrColour([id]) },
    { sep: true }, { t: 'Remove…', dim: x.kind === 'Audience' && same.length === 1, f: () => A.scrRemove([id]) }]; };
M.scrColour = ([id]) => { const x = byId(DOC.screens, id); return IDENT.map(([c, n]) => ({ t: n, on: x.tint === c, f: () => mutate(() => { x.tint = c; }) })); };
A.identify = () => { state.identify = !state.identify; render();  };
M.testPattern = () => [['bars', 'Colour bars'], ['grid', 'Grid'], ['white', 'White'], ['off', 'Off']].map(([k, n]) => ({ t: n, on: state.test === k, f: () => { state.test = k; render(); } }));
// Choosing where a screen goes. What it had for NDI or DeckLink is kept, for coming back; a DeckLink output starts at the
// card's own format unless one was chosen here.
function scrSetTo(x, v) { x.to = v; x.disconnected = !!(DISPLAYS.find(d => 'Display: ' + d.name === v) || {}).gone;
  if (v === 'NDI' && !ndiOf(x)) x.ndiName = 'SoftPresenter ' + x.name;
  if (isDeckLink(v) && x.formatFrom !== 'chosen') { const d = DECKLINK[v].def; x.format = d || '1080i59.94'; x.formatFrom = d ? 'card' : 'usual'; } }
C.scr = (v, [id, prop]) => mutate(() => { const x = byId(DOC.screens, id);
  if (/^edge\d$/.test(prop)) { x.edges = Array.isArray(x.edges) ? [...x.edges] : [0, 0, 0, 0]; x.edges[+prop[4]] = Math.max(0, Math.min(540, Math.round(+v) || 0)); return; }
  if (prop === 'name') { v = v.trim(); const twin = DOC.screens.find(y => y !== x && y.name.toLowerCase() === v.toLowerCase());      // names are how `sp` finds a screen, so each is used once
    if (!v || twin) { refuse(twin ? `Another screen is called “${twin.name}”` : 'A screen needs a name'); return render(); } }
  x[prop] = prop === 'theme' ? v || null : v;
  if (prop === 'format') x.formatFrom = 'chosen';
  if (prop === 'rate' && v === '29.97') delete x.rate;      // the usual rate isn't kept
  if (prop === 'to') scrSetTo(x, v); });
// a switch for one of a screen's settings: its buttons carry "id,prop=value"
const scrSwitch = (x, prop, list, def) => seg(list.map(([v, t]) => [`${x.id},${prop}=${v}`, t]), `${x.id},${prop}=${x[prop] || def}`, 'scrPick');
A.scrPick = ([id, pv]) => mutate(() => { const [prop, v] = pv.split('='); byId(DOC.screens, id)[prop] = v; if (prop === 'format') byId(DOC.screens, id).formatFrom = 'chosen'; });
A.scrSet = ([id, prop, ...v]) => mutate(() => { const sc = byId(DOC.screens, id); if (prop === 'to') scrSetTo(sc, v.join(',')); else sc[prop] = v.join(','); });
A.scrLayer = ([id, l]) => mutate(() => { const L = byId(DOC.screens, id).layers; L.includes(l) ? L.splice(L.indexOf(l), 1) : L.push(l); });
M.groupColour = ([n]) => ['#4285f4', '#cc4eb4', '#f4a000', '#34a86e', '#9b59b6', '#00acc1', '#78909c', '#8d6e63'].map(c => ({ t: c, on: DOC.groups[+n].color === c, f: () => mutate(() => { DOC.groups[+n].color = c; }) }));
A.groupEdit = ([n, prop]) => { const g = DOC.groups[+n]; ask(prop === 'key' ? `Key for ${g.name}` : 'Rename group', prop === 'key' ? 'A letter' : 'Name', g[prop], v => mutate(() => { const old = g.name; g[prop] = prop === 'key' ? v.slice(0, 1).toUpperCase() : v;
  if (prop === 'name') DOC.songs.forEach(s => { s.slides.forEach(sl => { if (sl.group === old) sl.group = v; }); s.arrangements.forEach(a => { a.groups = a.groups.map(x => x === old ? v : x); }); }); }), { allowEmpty: prop === 'key' }); };
function renameGroup(n, v) { const g = DOC.groups[n], old = g.name; g.name = v; DOC.songs.forEach(s => { s.slides.forEach(sl => { if (sl.group === old) sl.group = v; }); s.arrangements.forEach(a => { a.groups = a.groups.map(x => x === old ? v : x); }); }); }
C.groupName = (v, [n]) => { v = v.trim(); if (!v) return render(); mutate(() => renameGroup(+n, v)); };
A.groupDel = ([n]) => mutate(() => { DOC.groups.splice(+n, 1); });
A.groupAdd = () => ask('New group', 'Name', '', v => mutate(() => { DOC.groups.push({ name: v, color: '#8d6e63', key: '' }); }), { placeholder: 'Interlude' });

/* VoxLive */
A.voxDisconnect = () => confirmDo('Disconnect VoxLive?', 'The key is forgotten on this computer; it stays in VoxLive until you revoke it there. This can’t be undone.', 'Disconnect', () => { DOC.voxlive.connected = false; if (state.src.type === 'vox') state.src = { type: 'pl', id: DOC.playlists[0].id }; state.vox = null; render(); });
A.voxConnect = () => { const k = $('voxKey').value.trim(); if (k.length < 8) return trouble('VoxLive refused that key'); DOC.voxlive.connected = true; DOC.voxlive.key = k.slice(0, 12); render();  };
A.voxLook = ([id]) => { const sc = byId(DOC.voxlive.screens, id), l = state.sheet && state.sheet.vox === id ? state.sheet.look : clone(sc.look);
  openSheet({ title: `${sc.name} · look`, wide: true, body: html`<div class="bi-opts"><div>
      ${field('Text size', html`<input type="range" min="32" max="96" value="${l.size}" data-c="voxSet:size" style="width:100%">`)}
      ${field('Colour', html`<span class="ctl">${['#ffffff', '#ffc400', '#7ad7ff', '#34c56b'].map(c => html`<button class="swt ${l.colour === c ? 'on' : ''}" style="background:${c}" data-a="voxSetA:colour,${c}"></button>`)}</span>`)}
      ${field('Lines', html`<select class="sel" data-c="voxSet:lines">${options([1, 2, 3, 4], l.lines)}</select>`)}
      <label class="ui-check"><input type="checkbox" data-c="voxSet:background" ${{ checked: !!l.background }}> A band behind the words</label></div>
    <div class="bi-prev"><span class="cap">Preview · VoxLive’s free test mode</span><div class="thumb"><div class="bg" style="background:#04121c"></div><div class="lv-voxprev" style="font-size:${l.size / 19.2}cqw;color:${l.colour};${l.background ? 'background:rgba(0,0,0,.7)' : ''}">${Array.from({ length: l.lines }, (_, n) => [n ? BR : '', `Sample caption line ${n + 1}`])}</div></div></div></div>`,
    note: 'Nothing reaches the projector or starts a translation while adjusting',
    buttons: [{ t: 'Revert' }, { t: 'Save to VoxLive', primary: true, f: () => { sc.look = clone(state.sheet.look); toast(`${sc.name} changed wherever it is showing`); } }] });
  state.sheet.vox = id; state.sheet.look = l; };
C.voxSet = (v, [k], el) => { state.sheet.look[k] = el.type === 'checkbox' ? el.checked : +v; A.voxLook([state.sheet.vox]); };
A.voxSetA = ([k, v]) => { state.sheet.look[k] = v; A.voxLook([state.sheet.vox]); };

/* the first-start setup: setup.js */
A.outsideDemo = () => { state.changedOutside = DOC.songs[1].id; state.mode = 'show'; render(); };

/* keys that can be changed */
const KEY_NAMES = { next: 'Next slide', previous: 'Previous slide', clearAll: 'Clear all', clearSlide: 'Clear the slide', clearMedia: 'Clear the media', clearProps: 'Clear the props', clearAudio: 'Stop the audio', clearMessages: 'Clear the message', find: 'Find a song, a line or a verse' };
const KEY_DEFAULTS = clone(DOC.settings.keys);
const keyLabel = k => ({ ' ': 'Space', ArrowRight: '→', ArrowLeft: '←', ArrowUp: '↑', ArrowDown: '↓', Escape: 'Esc' })[k] || (k.length === 1 ? k.toUpperCase() : k);
A.keyChange = ([a]) => { openSheet({ title: KEY_NAMES[a], body: html`<p class="ui-p">Press the key to use. It is taken from anything else that had it.</p>`, buttons: [{ t: 'Cancel', f: () => { state.capture = null; } }] }); state.capture = a; };
function keyCaptured(k) {
  const a = state.capture; state.capture = null; closeSheet();
  if (['Shift', 'Meta', 'Control', 'Alt'].includes(k)) return refuse('Choose a key that isn’t a modifier');
  mutate(() => { const K = DOC.settings.keys; for (const x of Object.keys(K)) K[x] = K[x].filter(y => y !== k); K[a] = [k]; });
  
}
A.keysReset = () => mutate(() => { DOC.settings.keys = clone(KEY_DEFAULTS); });

/* ───────── the Screens page: cards by purpose ───────── */
// What a screen is for. Choosing one sets the technical settings for it; Fine-tune changes them one by one.
const PURPOSES = [['room', 'Room', 'What the congregation watches: everything, full screen'], ['stream', 'Stream over the camera', 'The words alone, see-through, for streaming software to lay over a camera'],
  ['switcher', 'Switcher, fill and key', 'The words and their shape for a video switcher, over DeckLink'], ['lobby', 'Lobby', 'Pictures, videos and announcements; no song words'], ['stage', 'Stage', 'For the people on stage: the words now, the next slide, the time']];
// A screen keeps what it is for once chosen. One saved before purposes existed is read from its settings, once, by the app.
const purposeOf = x => x.purpose !== undefined ? x.purpose : (x.kind === 'Stage' ? 'stage' : !x.layers.includes('slide') ? 'lobby' : x.keyOf || (isDeckLink(x.to) && (x.keyer || 'off') !== 'off') ? 'switcher' : x.bgMode === 'clear' ? 'stream' : 'room');
const purposeName = p => (PURPOSES.find(x => x[0] === p) || [, 'Not set up yet'])[1];      // a new screen's purpose is '' until one is chosen
function applyPurpose(x, p) {
  x.purpose = p;
  if (p === 'stage') { x.kind = 'Stage'; x.keyer = 'off'; x.layout = x.layout || DOC.stageLayouts[0].id; return; }      // a stage screen is never keyed
  x.kind = 'Audience'; x.colour = x.colour || '#000000'; x.edges = x.edges || [0, 0, 0, 0]; x.keyOf = ''; x.theme = x.theme === undefined ? null : x.theme;
  if (p === 'room') { x.bgMode = 'solid'; x.layers = [...ALL_LAYERS]; x.keyer = 'off'; }
  if (p === 'stream') { x.bgMode = 'clear'; x.layers = ['slide', 'props', 'messages']; x.keyer = 'off'; const lt = DOC.themes.find(t => /lower/i.test(t.name)); if (lt && !x.theme) x.theme = lt.id; }
  if (p === 'switcher') { x.bgMode = 'clear'; x.alpha = 'premultiplied'; x.layers = ['slide', 'props', 'messages']; x.keyer = 'fill'; if (!isDeckLink(x.to)) { x.to = Object.keys(DECKLINK)[0]; x.format = DECKLINK[x.to].def || '1080i59.94'; x.formatFrom = DECKLINK[x.to].def ? 'card' : 'usual'; x.disconnected = false; } }
  if (p === 'lobby') { x.bgMode = 'solid'; x.layers = ['media', 'announcements']; x.keyer = 'off'; }
}
A.scrPurpose = ([id, p]) => mutate(() => applyPurpose(byId(DOC.screens, id), p));
// What Fine-tune has changed from the settings its purpose gives, in a few words ("see-through", "no media"), so a card
// never claims "Room" for a screen that is no longer set up as one.
const PRESET = { room: { bg: 'solid', layers: ALL_LAYERS, keyer: 'off' }, stream: { bg: 'clear', layers: ['slide', 'props', 'messages'], keyer: 'off' },
  switcher: { bg: 'clear', layers: ['slide', 'props', 'messages'], keyer: 'fill' }, lobby: { bg: 'solid', layers: ['media', 'announcements'], keyer: 'off' } };
function purposeDiff(x) {
  const p = PRESET[purposeOf(x)]; if (!p || x.kind !== 'Audience') return [];
  const out = [], names = L => L.map(l => LAYER_NAMES[l].toLowerCase()).join(', '), keyScreen = purposeOf(x) === 'switcher' && x.keyOf;
  if (x.keyOf && !keyScreen) { const f = byId(DOC.screens, x.keyOf); out.push(`a key of ${f ? f.name : 'a screen that has gone'}`); }
  if (!x.keyOf && (x.bgMode || 'solid') !== p.bg) out.push(x.bgMode === 'clear' ? 'see-through' : 'solid');
  const less = p.layers.filter(l => !x.layers.includes(l)), more = x.layers.filter(l => !p.layers.includes(l));
  if (less.length) out.push('no ' + names(less)); if (more.length) out.push('with ' + names(more));
  if (isDeckLink(x.to) && !keyScreen && (x.keyer || 'off') !== p.keyer) out.push(x.keyer === 'fill' ? 'fill and key' : x.keyer === 'over' ? 'keyed over the card’s input' : 'keyer off');
  return out;
}
// The technical line a card shows with Details on: where it goes, in full, then how it draws.
function scrTech(x) {
  const D = dispOf(x), k = x.keyer || 'off', E = Array.isArray(x.edges) ? x.edges : [0, 0, 0, 0], th = x.theme && byId(DOC.themes, x.theme);
  const out = !x.to ? 'Nowhere yet' : D ? `${D} · ${(DISPLAYS.find(d => d.name === D) || {}).size || ''}` : x.to === 'A window' ? 'A window · 960 × 540'
    : x.to === 'NDI' ? `NDI “${ndiOf(x) || 'no name'}” · 1920 × 1080` : `${x.to} · ${DECKLINK[x.to].conn} · ${x.format}${k === 'fill' ? ' · fill and key' : k === 'over' ? ' · over the card’s input' : ''}`;
  if (x.kind !== 'Audience') return [out, `Layout: ${(byId(DOC.stageLayouts, x.layout) || DOC.stageLayouts[0]).name}`];
  const f = x.keyOf && byId(DOC.screens, x.keyOf), L = ALL_LAYERS.filter(l => x.layers.includes(l));
  const look = [x.keyOf ? `Key of ${f ? f.name : 'a screen that has gone'}` : x.bgMode === 'clear' ? `See-through, ${x.alpha || 'straight'}` : 'Solid',
    (th ? th.name : 'Main theme') + ((vs => vs.length ? ` (+ ${vs.join(', ')})` : '')(DOC.versions.filter(v => (th || themeFor(null)).templates.some(tp => tp.version === v.id)).map(v => v.name))), L.length === 5 ? 'every layer' : L.length ? L.map(l => LAYER_NAMES[l].toLowerCase()).join(', ') : 'no layers',
    E.some(Number) ? `edges ${E.join('·')}` : '', x.bgMode !== 'clear' && !x.keyOf && (x.colour || '#000000') !== '#000000' ? `colour ${x.colour}` : ''].filter(Boolean).join(' · ');
  return [out, look];
}
A.scrDetails = () => { DOC.settings.scrDetails = !DOC.settings.scrDetails; render(); };      // remembered, like the other views
A.scrOpen = ([id]) => { state.scrOpen = id || null; render(); };
// small drawings of each purpose, for its tile
const PURPOSE_PIC = { room: html`<i class="pp-l1"></i><i class="pp-l2"></i>`, stream: html`<i class="pp-chk"></i><i class="pp-band"><i></i></i>`,
  switcher: html`<span class="pp-half"><i class="pp-l1"></i></span><span class="pp-half key"><i class="pp-bar"></i></span>`, lobby: html`<i class="pp-photo"></i><i class="pp-card"></i>`, stage: html`<i class="pp-big"></i><i class="pp-next"></i><i class="pp-clock">10:42</i>` };
const DEST_ICON = { display: svg('M1.5 2.5h10v6.500h-10zM4.500 11h4M6.500 9v2', 1.2), window: svg('M1.500 2.500h10v8h-10zM1.500 4.800h10', 1.2), ndi: svg('M3 9.500a5 5 0 0 1 7 0M1.500 7.500a7.500 7.500 0 0 1 10 0M6.500 11h.01', 1.3),
  decklink: svg('M1.500 4h10v5h-10zM4 6.500h.01M6.500 6.500h.01M9 6.500h.01', 1.3), auto: svg('M6.500 1.500v2M6.500 9.500v2M1.500 6.500h2M9.500 6.500h2M3 3l1.400 1.400M8.600 8.600L10 10M3 10l1.400-1.400M8.600 4.400L10 3', 1.2) };
// where a screen can go, as tiles: each display the computer sees (never the one SoftPresenter's window is on), a window, NDI, each DeckLink output
// A DeckLink audience screen's keyer, first after its output (as built, 24 November, e81bed3: the owner found the old rows
// confusing, and the church's lower thirds came out see-through). The choice sets the rest: keyed is see-through with
// premultiplied edges, Off is solid. A line under it says what the chosen one does.
const KEYER_ABOUT = { off: 'No keying: the picture goes out whole, as a display shows it.',
  fill: 'Two cables to a switcher’s keyer: the picture on this connector, its key on the one paired with it. Only the words are keyed: the media and the themes’ backgrounds are left out.',
  over: 'The card lays the picture over the video coming into it. Only the words are keyed: the media and the themes’ backgrounds are left out.' };
function keyerRows(x) {
  const k = x.keyer || 'off';
  return html`${stLine('Keyer', seg([['off', 'Off'], ['fill', 'Fill and key'], ['over', 'Over the card’s input']].map(([v, t]) => [`${x.id},${v}`, t]), `${x.id},${k}`, 'scrKeyer'), KEYER_ABOUT[k], k === 'off' && noKeying(x.format) ? `Not at ${x.format}: the card sends it as YUV only` : '')}
    ${k !== 'off' ? stLine('Soft edges', scrSwitch(x, 'alpha', [['premultiplied', 'Premultiplied (usual)'], ['straight', 'Straight']], 'premultiplied'), 'How a see-through edge is sent: try Straight if the words’ edges look dark') : ''}`;
}
// The format, after the keyer; when Desktop Video Setup has another playable format, a button to take it
function formatRow(x) {
  const setup = DECKLINK[x.to].def, other = setup && setup !== x.format && FORMATS.includes(setup);
  return html`${stLine('Format', html`<select class="sel" data-c="scr:${x.id},format"><optgroup label="Can key">${options(FORMATS.filter(v => !noKeying(v)), x.format)}</optgroup><optgroup label="No keying: sent as YUV only">${options(FORMATS.filter(noKeying), x.format)}</optgroup></select>${other ? html`<button class="btn sm" data-a="scrPick:${x.id},format=${setup}">Use ${setup}</button>` : ''}`, formatFrom(x))}
    ${other ? html`<p class="sc-note warn">Desktop Video Setup has ${setup}. A switcher usually needs every input in its own format.</p>` : ''}`;
}
A.scrKeyer = ([id, v]) => mutate(() => { const x = byId(DOC.screens, id); if (x.kind !== 'Audience') return;
  x.keyer = v; x.keyOf = '';
  if (v === 'off') x.bgMode = 'solid'; else { x.bgMode = 'clear'; x.alpha = 'premultiplied'; } });      // either keyed choice starts at premultiplied
// An NDI screen's frame rate (as built, 24 November, 01bf142): 29.97 unless chosen, and kept only when it isn't 29.97
const NDI_RATES = ['29.97', '30', '59.94', '60', '25', '50'];
const NDI_RATE_HINT = 'How many pictures a second this screen sends. Choose the rate the equipment that shows it plays at: 29.97 or 59.94 for most video equipment where the mains are 60 Hz, 25 or 50 where they are 50 Hz. At another rate the picture pauses for a moment every few seconds.';
function destTiles(x) {
  const used = to => { const y = order().find(o => o.on && o.to === to); return y && y !== x ? `Used by ${y.name}` : ''; };
  const t = (to, icon, name, sub) => html`<button class="sc-dest ${x.to === to ? 'on' : ''}" data-a="scrSet:${x.id},to,${to}"><span class="ico">${DEST_ICON[icon]}</span><b>${name}</b><span>${sub}</span></button>`;
  return html`<div class="sc-dests">${DISPLAYS.filter(d => !d.main && (!d.gone || x.to === 'Display: ' + d.name)).map(d => t('Display: ' + d.name, 'display', d.name, d.gone ? 'Not plugged in' : used('Display: ' + d.name) || d.size))}
    ${t('A window', 'window', 'A window', '960 × 540')}${t('NDI', 'ndi', 'NDI', 'Over the network')}${Object.entries(DECKLINK).map(([n, d]) => t(n, 'decklink', n, used(n) || `DeckLink ${d.conn}`))}</div>
    ${x.to === 'NDI' ? stLine('NDI name', html`<input class="field" data-c="scr:${x.id},ndiName" value="${x.ndiName || ''}" aria-label="NDI name">`, 'What receivers see it as') : ''}
    ${x.to === 'NDI' ? stLine('Frame rate', html`<select class="sel" data-c="scr:${x.id},rate" aria-label="Frame rate" title="${NDI_RATE_HINT}">${options(NDI_RATES.map(r => [r, r]), x.rate || '29.97')}</select>`, 'Pictures a second, as the equipment that shows it plays') : ''}
    ${isDeckLink(x.to) && x.kind === 'Audience' ? keyerRows(x) : ''}
    ${isDeckLink(x.to) ? formatRow(x) : ''}`;
}
const scrStatus = x => { const s = scrState(x); return html`<span class="sc-st ${s.tone}"><i></i><span>${s.t}</span></span>${s.fix ? html`<button class="btn sm sc-fix" data-a="${s.fix[1]}" title="${s.fix[0]}">${s.fix[0].startsWith('Take it') ? 'Take it' : s.fix[0]}</button>` : ''}`; };
const destName = x => dispOf(x) || (!x.to ? 'Nowhere yet' : x.to === 'NDI' ? `NDI “${ndiOf(x) || 'no name'}”` : x.to);

// Every screen a big card; a card opens its setup as three numbered questions
function scrCardsHTML(top) {
  const x = state.scrOpen && byId(DOC.screens, state.scrOpen);
  if (x) { const aud = x.kind === 'Audience', p = purposeOf(x);
    return html`<div class="sc1-setup"><div class="sc1-back"><button class="btn sm" data-a="scrOpen:">‹ All screens</button><span class="sp"></span>${onoff(x.on, `scrOn:${x.id}`)}</div>
      <div class="sc1-head"><div class="thumb ${goingOut(x) ? 'live' : ''}">${scrPic(x)}</div><div class="sc1-hd"><div class="sc-nm"><i class="sc-chip" style="background:${x.tint}"></i><input data-c="scr:${x.id},name" value="${x.name}" aria-label="Name"></div>${scrStatus(x)}</div></div>
      <div class="sc1-q"><h3><span class="n">1</span>What is it for?</h3><div class="sc-purps">${PURPOSES.map(([k, n, d]) => html`<button class="sc-purp ${p === k ? 'on' : ''}" data-a="scrPurpose:${x.id},${k}"><span class="pp pp-${k}">${PURPOSE_PIC[k]}</span><b>${n}</b><span>${d}</span></button>`)}</div>${purposeDiff(x).length ? html`<p class="sc-note sc-chgnote">Fine-tune has changed ${purposeName(p)}’s settings: ${purposeDiff(x).join(', ')}.<button class="btn sm" data-a="scrPurpose:${x.id},${p}">Put ${purposeName(p)}’s settings back</button></p>` : ''}</div>
      <div class="sc1-q"><h3><span class="n">2</span>Where does it go?</h3>${destTiles(x)}${scrDestNote(x)}</div>
      <div class="sc1-q"><h3><span class="n">3</span>${aud ? 'How the words look' : 'What the stage sees'}</h3>${aud ? stLine('Theme', html`<select class="sel" data-c="scr:${x.id},theme" ${{ disabled: !!x.keyOf }}>${options([['', 'The main theme'], ...DOC.themes.map(t => [t.id, t.name])], x.theme || '')}</select>`, 'The same words, laid out by this theme on this screen')
        : html`<div class="sc-lays">${DOC.stageLayouts.map(l => html`<button class="sc-lay ${x.layout === l.id ? 'on' : ''}" data-a="scrSet:${x.id},layout,${l.id}"><div class="thumb">${stageHTML(l.id)}</div><span>${x.layout === l.id ? I.check : ''}${l.name}</span></button>`)}</div>`}</div>
      ${aud ? html`<details class="sc1-fine" ${{ open: !!state.scrFine }}><summary data-a="scrFine">Fine-tune <span class="sub">layers, see-through, key, colour, edges</span></summary>${scrFineHTML(x)}</details>` : ''}${scrFootHTML(x)}</div>`; }
  const card = x => { const s = scrState(x); return html`<div class="sc1-card ${goingOut(x) ? 'on' : ''}" data-scr="${x.id}" data-m="scrRow:${x.id}"><button class="sc1-open" data-a="scrOpen:${x.id}" aria-label="Set up ${x.name}"><div class="thumb">${scrPic(x)}</div>${x.kind === 'Audience' && order()[0] === x ? html`<span class="sc-tag sc1-main" title="The first audience screen. A stage layout’s Screen Picture shows this one unless it names another.">Main</span>` : ''}</button>
    <div class="sc1-meta"><div class="sc1-t"><i class="sc-chip" style="background:${x.tint}"></i><b>${x.name}</b><span class="sp"></span>${onoff(x.on, `scrOn:${x.id}`)}</div>
      <span class="sc-st ${s.tone}"><i></i><span>${s.t}</span></span><span class="sc1-what">${purposeName(purposeOf(x))}${purposeDiff(x).length ? html` <i class="sc-chg" title="Fine-tune has changed it: ${purposeDiff(x).join(', ')}">changed</i>` : ''} · ${destName(x)}</span>${DOC.settings.scrDetails ? html`<span class="sc1-tech">${scrTech(x).map(l => html`<span>${l}</span>`)}</span>` : ''}${s.fix ? html`<button class="btn sm sc-fix" data-a="${s.fix[1]}">${s.fix[0].startsWith('Take it') ? 'Take it' : s.fix[0]}</button>` : ''}</div></div>`; };
  return html`${top}<div class="sc1-allsw"><span class="sub">All audience screens</span>${onoff(state.audience, 'out:audience')}<span class="sub">All stage screens</span>${onoff(state.stage, 'out:stage')}<span class="sp"></span><span class="sub" title="A technical line on every card: the output in full, its format and keyer, see-through or key, theme, layers, edges">Details</span>${onoff(DOC.settings.scrDetails, 'scrDetails')}</div>
    <div class="sc1-grid">${order().map(card)}<button class="sc1-add" data-a="scrAddAsk"><span>${I.plus}</span><b>Add a screen</b><span class="sub">Choose what it is for and where it goes</span></button></div>`;
}
A.scrFine = (_, el, ev) => { if (ev) ev.preventDefault(); state.scrFine = !state.scrFine; render(); };
A.scrAddAsk = () => A.scrAdd(['Audience']);      // what it is for (stage included) is the setup's first question

/* ═════════ setup.js ═════════ */
// The first-run setup and the song catalog it brings (the owner, 23 November: "i want it to look nice and very simple";
// "make sure all the importatn things are in the onbaording so when theyre done its ready for them to use").
// Five steps: Languages (with the welcome), Songs, Bibles, Screens, Ready. Each can be skipped; every answer is the same one
// Settings makes, and the setup can be opened again from the SoftPresenter menu. Approved by the owner on 23 November.
'use strict';

/* ───────── the song catalog (Worship Leader App) ─────────
   Songs downloaded from it are kept apart from the church's own: found in the find box and the chooser, marked with where they
   came from, and joining the library when one goes into a playlist or is edited. Untouched songs update quietly; anything the
   church changed stays changed. */
const CAT_LANGS = Object.keys(CATALOG.langs);
I.catalog = svg('M3.500 2h7v8.500h-7zM1.800 3.800v8h6.500M5.500 4.500h3M5.500 6.500h3', 1.2);      // a stack of pages: a collection to look in (the name says whose)
const catState = () => state.cat || (state.cat = { have: {}, dl: {}, fail: {}, offline: false });      // nothing downloaded in the sample until the owner approves the design; ?demo=cat… turns it on
const catSample = () => { catState().have = { ru: { at: Date.now() - 3 * 864e5 }, en: { at: Date.now() - 3 * 864e5 } }; };      // the sample church keeps Russian and English
const catHave = () => catState().have;
const catOn = () => CATALOG.songs.length > 0 && CAT_LANGS.some(l => catHave()[l]);
const catCount = () => CAT_LANGS.filter(l => catHave()[l]).reduce((n, l) => n + CATALOG.langs[l].songs, 0);
const fmtN = n => n.toLocaleString('en-GB');
const langName = l => CATALOG.langs[l] ? CATALOG.langs[l].name : l;
const CAT_CACHE = {};
// A catalog song as a song to look at (not in the library): its own words only.
function catSong(key) {
  if (CAT_CACHE[key]) return CAT_CACHE[key];
  const c = CATALOG.songs.find(x => x.key === key); if (!c) return null;
  const s = song(c.title, c.groups.map(([, g, w]) => [g, [w, '']]));
  s.id = 'wla-' + key; s.cat = { key, no: c.no || null, lang: c.lang };
  return (CAT_CACHE[key] = s);
}
const catSongs = () => CATALOG.songs.filter(c => catHave()[c.lang]).map(c => catSong(c.key));
const catOf = s => s && (s.cat || s.from) ? CATALOG.songs.find(c => c.key === (s.cat || s.from).key) : null;
// The other language comes along group by group, where both versions have the group (Verse 1 with Verse 1); a group with no
// partner shows one language. Returns the slides' second lines, and the groups left with one language.
function catLineUp(c, lang) {
  const other = c.links && CATALOG.songs.find(x => x.key === c.links[lang]); if (!other) return null;
  const theirs = Object.fromEntries(other.groups.map(([code, , w]) => [code, w]));
  return { lines: c.groups.map(([code]) => theirs[code] || ''), alone: c.groups.filter(([code]) => !theirs[code]).map(([, g]) => g), other };
}
// Into the library: a copy of its own, in a library named after the catalog, with the second language if the church shows one.
function catJoin(key) {
  const had = DOC.songs.find(s => s.from && s.from.key === key); if (had) return had;
  const c = CATALOG.songs.find(x => x.key === key), langs = DOC.settings.langs || [], below = langs.find(l => l !== c.lang);
  const up = below && DOC.settings.catLinked !== false ? catLineUp(c, below) : null;
  const s = song(c.title, c.groups.map(([, g, w], n) => [g, [w, up ? up.lines[n] : '']]));
  s.from = { key, no: c.no || null, lang: c.lang, added: Date.now(), changed: false, newer: false, alone: up && up.alone.length ? { lang: below, groups: up.alone } : null, with: up ? below : null };
  DOC.songs.push(s);
  let lib = DOC.libraries.find(l => l.catalog); if (!lib) { lib = { id: newId('lib'), name: CATALOG.name, songs: [], catalog: true }; DOC.libraries.push(lib); }
  lib.songs.push(s.id);
  return s;
}
// A song the church edits stops updating from the catalog (it is never overwritten); the Edit page calls this through mutate.
const catTouched = s => { if (s && s.from && !s.from.changed) s.from.changed = true; };

// The catalog in the chooser and the left column: songs to find, never mixed into the church's own.
function catLeft(hit) {
  const list = catSongs().filter(s => hit(s.title) || s.slides.some(sl => hit(allWords(sl))) || (s.cat.no && hit(String(s.cat.no))));
  const rows = list.length ? list.map(s => html`<div class="item"><button class="rowbtn ${state.openCat === s.id ? 'on' : ''}" data-a="openCat:${s.id}" data-m="catSong:${s.cat.key}" draggable="true" data-drag="song:${s.id}" title="Drag it onto a playlist or library in the chooser"><span class="ico">${I.catalog}</span><span class="t">${s.title}</span><span class="meta mono">${s.cat.lang.toUpperCase()}</span></button></div>`)
    : html`<div class="sh-none">No songs match</div>`;
  return { rows: [html`<p class="cat-note">Free songs to find. One joins your library when you add it, put it in a playlist or edit it.</p>`, rows],
    add: html`<button class="btn sh-add" data-a="catSettings">Languages and updates…</button>` };
}
M.catSong = ([key]) => [{ t: 'Add to Library', f: () => A.catAdd([key]) }, { head: 'Add to Playlist' }, ...DOC.playlists.map(p => ({ t: p.name, f: () => catToPlaylist(key, p.id) }))];
function catToPlaylist(key, pid) { let s; const p = byId(DOC.playlists, pid); mutate(() => { s = catJoin(key); p.items.push({ id: newId('it'), kind: 'song', song: s.id }); }); toast(`Added ${s.title} to ${p.name}${s.from.with ? `, with its ${CATALOG.langs[s.from.with].en} version below` : ''}.`); }
// A catalog song's slides: to look at, put on screen, or take into the library; changing one starts with the library.
const slideMenu0 = M.slide;
M.slide = ([key, s]) => !String(key).startsWith('cat-') ? slideMenu0([key, s]) : [{ t: 'Add Song to Library', f: () => A.catAdd([key.slice(8)]) }, { t: 'Edit (it joins your library)', f: () => A.editSlide([key, s]) }];
A.openCat = ([id]) => { state.openCat = id; deckCache = null; render(); };
A.catSettings = () => { state.mode = 'settings'; state.setTab = 'integrations'; render(); };
// The title bar of a catalog song: where it is from, in words, and the way into the library.
function catBar(d) {
  const s = d.song, c = catOf(s); if (!c) return '';
  const others = Object.keys(c.links || {}).filter(l => catHave()[l]);
  return html`<div class="cat-bar"><span class="cat-src">${I.catalog} From ${CATALOG.name}</span>${others.length ? html`<span class="sub">Also in ${others.map(langName).join(', ')}</span>` : ''}${c.chords ? html`<span class="sub" title="Chords are kept with the song; slides never show them">Has chords</span>` : ''}<span class="sp"></span><button class="btn sm" data-a="menu:catSong,${c.key}">${I.plus} Add to… ▾</button></div>`;
}
A.catAdd = ([key]) => { let s; mutate(() => { s = catJoin(key); }); toast(`“${s.title}” is in your library now${s.from.with ? `, with its ${CATALOG.langs[s.from.with].en} version below` : ''}`); };
// A library song that came from the catalog: a quiet line under its title bar, only when there is something to say.
function catLine(s) {
  if (!s || !s.from) return '';
  const bits = [];
  if (s.from.alone) bits.push(`${s.from.alone.groups.join(', ')} ${s.from.alone.groups.length === 1 ? 'shows' : 'show'} ${CATALOG.langs[s.from.lang].en} only: the ${CATALOG.langs[s.from.alone.lang].en} version has no ${s.from.alone.groups.length === 1 ? 'such group' : 'such groups'}`);
  if (s.from.changed && s.from.newer) bits.push(`A newer version is in ${CATALOG.name}; yours is kept`);
  return bits.length ? html`<div class="cat-line">${I.catalog}<span>${bits.join(' · ')}</span>${s.from.changed && s.from.newer ? html`<button class="btn sm" data-a="songInfo:${s.id}">Look at it</button>` : ''}</div>` : '';
}
// In a song's details: where it came from, with the site, and what updating does to it.
function catInfo(s) {
  if (!s || !s.from) return '';
  const when = new Date(s.from.added).toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' });
  return html`<div class="cat-info"><div class="cat-src">${I.catalog} From ${CATALOG.name}</div>
    <p class="cat-p">Added ${when}${s.from.no ? html` · <span title="The catalog’s own number for it">№ ${s.from.no}</span>` : ''}${s.from.with ? ` · with its ${CATALOG.langs[s.from.with].en} version` : ''}</p>
    <p class="cat-p">${s.from.changed ? `Changed here, so it is never updated from ${CATALOG.name}.` : `Kept up to date from ${CATALOG.name} until it is changed here.`}</p>
    ${s.from.changed && s.from.newer ? html`<div class="cat-newer"><span>A newer version is in ${CATALOG.name}. Yours stays as it is unless you choose it.</span><button class="btn sm" data-a="catNewer:${s.id}">Use the Newer Version</button></div>` : ''}
    <button class="btn sm" data-a="catSite" title="Opens ${CATALOG.site} in your browser">${CATALOG.site} ↗</button></div>`;
}
A.catSite = () => toast(`${CATALOG.site} opens in your browser`);      // the app opens it; this page stays on its own address
A.catNewer = ([id]) => { closeSheet(); mutate(() => { const s = songById(id); s.from.newer = false; s.from.changed = false; }); toast('The newer version is in; Undo brings yours back'); };

// Find: the church's own songs first, then the downloaded songbooks' as their own group, then Bible verses (as built,
// 24 November, ab23fcd). Ranked: the title is the words, starts with them, has them, then the first line has them (then
// any other line); the church's languages first within each rank. A song already in the library is left out. As many as
// the church's songs leave room for among six, always at least two. A bare number finds the catalog's number for a song.
function catFind(want, room = 4) {
  if (!catOn() || want.length < 2) return [];
  const num = /^\d+$/.test(want) ? +want : null, church = DOC.settings.langs || [], found = [];
  for (const c of CATALOG.songs) {
    if (!catHave()[c.lang] || DOC.songs.some(s => s.from && s.from.key === c.key)) continue;
    const s = catSong(c.key), title = s.title.toLowerCase(), first = slideWords(s.slides[0]).split('\n')[0];
    const k = num != null ? -1 : s.slides.findIndex(sl => [s.title, sl.group, allWords(sl)].join('\n').toLowerCase().includes(want));
    const rank = num != null ? (c.no === num ? 0 : -1) : title === want ? 0 : title.startsWith(want) ? 1 : title.includes(want) ? 2 : first.toLowerCase().includes(want) ? 3 : k >= 0 ? 4 : -1;
    if (rank < 0) continue;
    const at = church.indexOf(c.lang);
    found.push({ c, s, k, first, rank, lang: at < 0 ? church.length : at });
  }
  found.sort((a, b) => a.rank - b.rank || a.lang - b.lang);
  const out = found.slice(0, room).map(({ c, s, k, first }) => html`<button class="fd-row cat" data-a="findCat:${s.id},${Math.max(0, k)}" draggable="true" data-drag="find:cat,${c.key}" title="From ${CATALOG.name}. Drag it into the playlist, or open it"><i class="fd-ico">${I.note}</i><b>${s.title}</b><span class="mono">${num != null ? `№ ${c.no} · ` : 'Song · '}${CATALOG.langs[c.lang].name}</span><em>${first}</em></button>`);
  return out.length ? [html`<div class="fd-sect cap">From ${CATALOG.name}</div>`, ...out] : [];
}
A.findCat = ([id]) => { $('find').value = ''; $('found').hidden = true; $('find').blur(); state.mode = 'show'; state.src = { type: 'cat', id: 'wla' }; state.openCat = id; state.srcMenu = false; deckCache = null; render(); };

/* ───────── the setup ───────── */
const SU_STEPS = ['Languages', 'Songs', 'Bibles', 'Screens', 'Ready'];
// The Bibles the app has, by language (the app's list, 23 November): built in, or downloaded with Continue.
const SU_BIBLES = {
  ru: [['syn', 'Синодальный перевод', true]],
  en: [['kjv', 'King James Version', true], ['web', 'World English Bible', true], ['bsb', 'Berean Standard Bible', true], ['asv', 'American Standard Version', false], ['ylt', 'Young’s Literal Translation', false], ['dby', 'Darby Translation', false], ['gnv', 'Geneva Bible 1599', false]],
  uk: [['ukr1905', 'Переклад Куліша та Пулюя', false], ['npu', 'Новий переклад українською (Новий Завіт і Псалми)', false]],
};
const suBible = id => { for (const l of CAT_LANGS) { const b = SU_BIBLES[l].find(x => x[0] === id); if (b) return { id, name: b[1], built: b[2], lang: l }; } return null; };
const SU_DISPLAYS = [['HP 27f', '1920 × 1080'], ['3DTV', '3840 × 2160'], ['Stage TV', '1920 × 1080']];      // the sample's displays; the main one (with the menu bar) is never offered

function suNew(at = 0) {
  const sys = (navigator.languages || ['en']).map(l => l.slice(0, 2)), had = DOC.settings.langs && DOC.settings.langs.length ? [...DOC.settings.langs] : null;
  const langs = had || (CAT_LANGS.filter(l => sys.includes(l)).length ? CAT_LANGS.filter(l => sys.includes(l)) : ['en']);
  const pick = l => (SU_BIBLES[l] || [])[0] ? SU_BIBLES[l][0][0] : null;
  state.su = { step: at, again: !!DOC.settings.setupDone, langs, songs: Object.fromEntries(langs.map(l => [l, !catHave()[l]])), others: false, looked: false, skipped: {},
    top: pick(langs[0]), below: langs[1] ? pick(langs[1]) : null, aud: 'HP 27f', stage: 'Stage TV' };
}
A.setup = ([step]) => { suNew(+(step || 0)); suOpen(); };
function suOpen() {
  const u = state.su, n = u.step, last = n === SU_STEPS.length - 1;
  const ticked = n === 1 ? u.langs.concat(u.others ? CAT_LANGS.filter(l => !u.langs.includes(l)) : []).filter(l => u.songs[l] && !catHave()[l] && !catState().offline && CATALOG.langs[l].songs) : [];
  const dlBible = n === 2 ? [u.top, u.below].map(suBible).filter(b => b && !b.built) : [];
  u.seen = Math.max(u.seen || 0, n);
  const go = n === 0 && !u.again ? 'Get Started' : last ? 'Start Using SoftPresenter' : ticked.length || dlBible.length ? 'Download and Continue' : 'Continue';
  openSheet({ title: 'Setup', wide: true, cls: 'su', body: html`${suBar(n)}${suBody(n)}`,
    note: html`<span id="suProgress" class="su-prog">${suProgress()}</span>`,
    buttons: [n === 0 ? { t: u.again ? 'Close' : 'Skip Setup', f: () => { if (!u.again) toast('The setup is in the SoftPresenter menu whenever you want it'); } } : { t: 'Back', keep: true, f: () => suGo(n - 1) },
      ...(n > 0 && !last ? [{ t: 'Skip', keep: true, f: () => { u.skipped[n] = true; suGo(n + 1); } }] : last ? [{ t: 'Show a Test Slide', keep: true, f: suTest }] : []),
      { t: go, primary: true, keep: !last, f: () => { if (last) return suFinish(); if (n === 0 && !u.langs.length) { refuse('Choose at least one language'); return false; } delete u.skipped[n]; if (n === 1) ticked.forEach(catDownload); if (n === 0) suLangsDone(); suGo(n + 1); } }] });
  if (document.activeElement && $('sheet').contains(document.activeElement)) document.activeElement.blur();      // a question to read first, not a field to type in
}
const suGo = n => { state.su.step = Math.max(0, Math.min(SU_STEPS.length - 1, n)); suOpen(); const b = $('sheet').querySelector('.ui-sb'); if (b) b.scrollTop = 0; };
// The steps along the top: the one now in ink, those done with a tick, those skipped with a dash.
const suBar = n => html`<div class="su-bar">${SU_STEPS.map((t, i) => html`<button class="su-dot ${i === n ? 'on' : i < n ? (state.su.skipped[i] ? 'skip' : 'done') : ''}" data-a="suStep:${i}" ${{ disabled: i > (state.su.seen || 0) || (!state.su.langs.length && i > 0) }}><i>${i < n && !state.su.skipped[i] ? I.check : i + 1}</i>${t}</button>`)}</div>`;
A.suStep = ([i]) => { if (state.su.step === 0 && +i > 0) { if (!state.su.langs.length) return refuse('Choose at least one language'); suLangsDone(); } suGo(+i); };      // only to steps already reached
const suHead = (t, line) => html`<h2 class="su-t">${t}</h2>${line ? html`<p class="su-line">${line}</p>` : ''}`;

function suBody(n) {
  const u = state.su;
  if (n === 0) {
    const card = l => { const on = u.langs.includes(l), L = CATALOG.langs[l];
      return html`<button class="su-lang ${on ? 'on' : ''}" data-a="suLang:${l}" aria-pressed="${on ? 'true' : 'false'}"><i class="su-tick">${on ? I.check : ''}</i><b>${L.name}</b><span>${L.en !== L.name ? L.en + ' · ' : ''}${L.songs ? `${fmtN(L.songs)} songs` : 'No songs yet'} · ${SU_BIBLES[l].length === 1 ? 'a Bible' : `${SU_BIBLES[l].length} Bibles`}</span></button>`; };
    const sel = (which, now) => html`<select class="sel" data-c="suOrder:${which}">${options(u.langs.map(l => [l, langName(l)]), now)}</select>`;
    // The app's own words come first, so the rest of the setup is read in them (as built, 24 November)
    return html`${suHead(u.again ? 'Set up SoftPresenter' : 'Welcome to SoftPresenter', u.again ? 'Your answers are filled in. Change what you like; nothing else changes.' : 'A few questions and it’s ready for Sunday. Every answer can be changed later.')}
      <div class="su-ui"><span class="cap">Language of SoftPresenter</span>${uiLangMenu()}</div>
      <h3 class="su-q">Which languages does your church show?</h3>
      <div class="su-langs">${CAT_LANGS.map(card)}</div>
      ${u.langs.length > 1 ? html`<div class="su-order"><span class="cap">On screen</span>${sel(0, u.langs[0])}<span class="su-w">on top,</span>${sel(1, u.langs[1])}<span class="su-w">below</span></div>
        <p class="su-small">Songs and Bible passages show two languages, one above the other.${u.langs.length > 2 ? ` ${langName(u.langs[2])} songs and Bibles are there too, for when you want them.` : ''}</p>`
        : html`<p class="su-small">${u.langs.length ? 'One language on screen. Choose a second for songs and passages in two.' : 'Choose at least one.'} More languages are coming.</p>`}`;
  }
  if (n === 1) {
    const st = catState(), shown = u.langs.concat(u.others ? CAT_LANGS.filter(l => !u.langs.includes(l)) : []);
    const row = l => { const L = CATALOG.langs[l], have = catHave()[l], dl = st.dl[l], none = !L.songs;
      if (none) return html`<div class="su-row off"><span class="su-chk"></span><b>${L.name}</b><span class="sub">No songs in this language yet</span></div>`;
      if (have) return html`<div class="su-row"><i class="su-have">${I.check}</i><b>${L.name}</b><span class="su-n mono">${fmtN(L.songs)} songs</span><span class="sub">On this computer · checked ${suAgo(have.at)}</span><span class="sp"></span><button class="btn sm" data-a="say:Checking ${L.en} for new songs (the app’s monthly check, done now)">Update Now</button><button class="btn sm" data-a="suRemove:${l}">Remove</button></div>`;
      return html`<label class="su-row ${st.offline ? 'off' : ''}"><input type="checkbox" data-c="suSongs:${l}" ${{ checked: !!u.songs[l] && !st.offline, disabled: st.offline }}><b>${L.name}</b><span class="su-n mono">${fmtN(L.songs)} songs</span><span class="sub">${dl ? `Downloading · ${dl.pct}%` : `${L.mb} to download · ${L.disk} on this computer`}</span></label>`; };
    const below = u.langs[1];
    return html`${suHead('Songs', 'Free songs to search, and the songs you already have.')}
      <div class="su-card"><div class="su-src">${I.catalog}<b>Songs from ${CATALOG.name}</b><button class="btn sm su-site" data-a="catSite">${CATALOG.site} ↗</button></div>
        <p class="su-small">A free songbook. The songs are kept on this computer, so finding one needs no internet, and they stay apart from your own songs until you use one.</p>
        ${st.offline ? html`<div class="su-warn"><i class="nk warn"></i><span>Not connected to the internet. The songs can be downloaded later, here or in Settings › Integrations.</span><button class="btn sm" data-a="suRetry">Try Again</button></div>` : ''}
        <div class="su-rows">${shown.map(row)}</div>
        ${!u.others && CAT_LANGS.some(l => !u.langs.includes(l)) ? html`<button class="su-more" data-a="suOthers">Other languages…</button>` : ''}
        ${below ? html`<label class="su-row su-opt"><input type="checkbox" data-c="suLinked" ${{ checked: DOC.settings.catLinked !== false }}><span>A song’s ${CATALOG.langs[below].en} version comes along as the second line, group by group, when it has one</span></label>` : ''}</div>
      <div class="su-card su-own"><div><b>Songs you already have</b><p class="su-small">${u.looked ? 'Found on this computer: 4,372 songs in 6 libraries. Each comes in as a library of the same name.' : `A library on this computer, or song files: text, ChordPro${isMac() ? ', Word' : ''} and others.`      /* only a Mac reads Word files (as built, 24 November) */}</p>${u.looked || !isMac() ? '' : html`<p class="su-small su-os">macOS asks whether SoftPresenter may use your Documents folder: choose OK.</p>`}</div>
        <div class="su-own-b">${u.looked ? html`<button class="btn" data-a="say:4,372 songs would come in, in 6 libraries">Bring Them In</button>` : html`<button class="btn" data-a="suLook">Look on This Computer</button>`}<button class="btn" data-a="say:Song files would be chosen here">Choose Files…</button></div></div>`;
  }
  if (n === 2) {
    const opts = now => html`${u.langs.map(l => html`<optgroup label="${langName(l)}">${options(SU_BIBLES[l].map(([id, name]) => [id, name]), now)}</optgroup>`)}`;
    const row = (label, which, now) => { const b = suBible(now);
      return html`<div class="su-bib"><span class="cap">${label}</span><select class="sel" data-c="suBible:${which}">${which === 'below' ? html`<option value="" ${{ selected: !now }}>None (one translation)</option>` : ''}${opts(now)}</select><span class="sub">${!b ? '' : b.built ? 'On this computer' : 'Downloads with Continue'}</span></div>`; };
    return html`${suHead('Bibles', u.langs.length > 1 ? 'Passages show two translations, one above the other, in the languages you chose.' : 'The translation passages show in. A second can go below it.')}
      <div class="su-card">${row('On top', 'top', u.top)}${row('Below', 'below', u.below)}
        ${[u.top, u.below].includes('npu') ? html`<p class="su-small">The New Ukrainian Translation has the New Testament and the Psalms only.</p>` : ''}</div>
      ${suBiblePreview()}
      <p class="su-small">More translations, and a Bible file you have, are added in the Bible page’s Options.</p>`;
  }
  if (n === 3) {
    const card = (which, name, size, now) => html`<button class="su-scr ${now === name ? 'on' : ''}" data-a="suScreen:${which},${name}"><i class="su-scr-pic">${name.startsWith('In a window') ? '▭' : name.startsWith('No') || name === 'Not now' ? '—' : ''}</i><b>${name}</b><span>${size}</span></button>`;
    // Only one display (as built, 24 November): it is where the operator works, so the audience never covers it. The screen
    // waits in a window and goes to a TV or projector the moment one is plugged in; NDI is offered when it is installed.
    if (u.one) { const ndi = ndiState() !== 'none';
      return html`${suHead('Screens', `Only one display is connected (${u.one}). This is where you work, so the audience screen never covers it: the congregation sees it on a TV or projector.${ndi ? ' It can also be sent over NDI, to a stream or a video switcher.' : ''}`)}
        <h3 class="su-q">The congregation sees</h3>
        <div class="su-scrs">${card('aud', 'In a window for now', 'On a TV or projector as soon as one is plugged in', u.aud)}${ndi ? card('aud', 'Send over NDI', 'For a stream or a video switcher', u.aud) : ''}${card('aud', 'Not now', 'Choose in Settings later', u.aud)}</div>`; }
    return html`${suHead('Screens', 'Where the slides go. A screen goes there at once, so you can check.')}
      <h3 class="su-q">The congregation sees</h3>
      <div class="su-scrs">${SU_DISPLAYS.filter(([d]) => d !== u.stage || d === u.aud).map(([d, sz]) => card('aud', d, sz, u.aud))}${card('aud', 'In a window', 'Move it to a screen later', u.aud)}${card('aud', 'Not now', 'Choose in Settings later', u.aud)}</div>
      <h3 class="su-q">The band sees <span class="su-w">a stage screen, if you have one</span></h3>
      <div class="su-scrs">${SU_DISPLAYS.filter(([d]) => d !== u.aud).map(([d, sz]) => card('stage', d, sz, u.stage))}${card('stage', 'No stage screen', '', u.stage)}</div>
      <div class="su-id"><button class="btn" data-a="identify">Identify Screens</button><span class="su-small">Shows each screen’s name on it. ${isMac() ? 'Your main display, with the menu bar, keeps' : 'The main display keeps'} SoftPresenter’s own window.</span></div>`;
  }
  // Ready: what is set, with a way back to each; what was skipped or still needs doing says so.
  const st = catState(), dl = Object.keys(st.dl), fail = Object.keys(st.fail), kept = CAT_LANGS.filter(l => catHave()[l]);
  const item = (i, label, text, warn = false, act = null, none = false) => html`<div class="su-done ${warn ? 'warn' : ''}"><i class="su-di">${warn ? html`<i class="nk warn"></i>` : none ? '–' : I.check}</i><span class="cap">${label}</span><span class="su-dt">${text}</span>${act || html`<button class="btn sm" data-a="suStep:${i}">Change</button>`}</div>`;
  const songsText = fail.length ? html`<span>${fail.map(l => `${CATALOG.langs[l].en} songs didn’t download: ${st.fail[l]}`).join('; ')}.</span>`
    : kept.length || dl.length ? html`<span>${fmtN(kept.concat(dl).reduce((n, l) => n + CATALOG.langs[l].songs, 0))} songs from ${CATALOG.name}${dl.length ? html` · <b class="su-pct">${Math.round(dl.reduce((n, l) => n + st.dl[l].pct, 0) / dl.length)}%</b>` : ''}</span>` : 'None yet. Your own songs, or the catalog’s, can come in at any time.';
  const prev = suPreview();
  return html`${suHead('Ready for Sunday', 'Here is what is set. Everything can be changed in Settings at any time.')}
    <div class="su-ready"><div class="su-list">
      ${item(0, 'Languages', u.langs.length > 1 ? `${langName(u.langs[0])} on top, ${langName(u.langs[1])} below` : langName(u.langs[0] || 'en'))}
      ${item(1, 'Songs', songsText, fail.length > 0, fail.length ? html`<button class="btn sm" data-a="suRetry">Try Again</button>` : null, !fail.length && !kept.length && !dl.length)}
      ${item(2, 'Bibles', u.skipped[2] ? 'The usual one: King James Version' : [u.top, u.below].map(suBible).filter(Boolean).map(b => b.name).join(', '))}
      ${u.skipped[3] || u.aud === 'Not now' ? item(3, 'Screens', 'No screen chosen yet: slides won’t show anywhere until one is.', true, html`<button class="btn sm" data-a="suStep:3">Choose</button>`)
        : item(3, 'Screens', `The congregation: ${u.aud}${u.stage && !u.stage.startsWith('No') ? ` · the band: ${u.stage}` : ''}`)}
    </div><figure class="su-prev"><div class="su-prev-pic">${prev}</div><figcaption class="su-small">How songs will look. Change the look in Edit › Themes.</figcaption></figure></div>
    <p class="su-tips"><b>Click a slide</b> to put it on screen, <b>Space</b> for the next, <b>Esc</b> to clear. Later, in Settings: <b>Remote Control</b>, to run the slides from a phone.</p>`;
}
// John 3:16 in the translations chosen, as passages will show (one still to download shows once it is in).
function suBiblePreview() {
  const u = state.su, chosen = [u.top, u.below].filter(Boolean), ids = chosen.filter(id => BIBLE_DATA.bibles[id] && bibleText(id, 'John', 3, 16)), one = ids.length === 1;
  if (!ids.length) return '';
  const objs = ids.flatMap((id, n) => [{ id: 'suv' + n, kind: 'text', name: 'Verse', role: n ? 'verse2' : 'verse1', text: bibleText(id, 'John', 3, 16), opacity: 1, ...(one ? { frame: { x: 130, y: 130, w: 1660, h: 660 }, style: { size: 60 } } : {}) },
    { id: 'sur' + n, kind: 'text', name: 'Reference', role: n ? 'ref2' : 'ref1', text: refTitle({ book: 'John', chapter: 3, from: 16, to: 16 }, id), opacity: 1, ...(one ? { frame: { x: 130, y: 820, w: 1660, h: 100 }, style: { size: 46 } } : {}) }]);
  const later = chosen.filter(id => !ids.includes(id)).map(id => (suBible(id) || {}).name).filter(Boolean);
  return html`<figure class="su-prev su-bprev"><div class="su-prev-pic">${thumb({ id: 'su-bible', group: '', label: '', objects: objs, bible: true }, { kind: 'bible' })}</div><figcaption class="su-small">John 3:16, as passages will show${later.length ? ` (${later.join(', ')} once downloaded)` : ''}.</figcaption></figure>`;
}
const isMac = () => APPLE;      // ?keys=pc and ?os= show the other systems
const suAgo = t => { const d = Math.round((Date.now() - t) / 864e5); return d <= 0 ? 'today' : d === 1 ? 'yesterday' : `${d} days ago`; };
// A slide in the chosen languages, from the sample catalog song.
function suPreview() {
  const u = state.su, c = CATALOG.songs.find(x => x.key === ({ ru: 'ru1', en: 'en1', uk: 'uk1' }[u.langs[0] || 'en'])), up = u.langs[1] ? catLineUp(c, u.langs[1]) : null;
  return thumb(slide(c.groups[1][1], c.groups[1][2], up ? up.lines[1] : ''), { kind: 'songs' });
}
const suProgress = () => { const st = catState(), dl = Object.keys(st.dl); if (!dl.length) return '';
  return html`<i class="su-dl">↓</i>${dl.map(l => `${CATALOG.langs[l].en} songs ${st.dl[l].pct}%`).join(' · ')}`; };

A.suLang = ([l]) => { const u = state.su; u.langs = u.langs.includes(l) ? u.langs.filter(x => x !== l) : [...u.langs, l]; if (!(l in u.songs)) u.songs[l] = !catHave()[l]; suOpen(); };
C.suOrder = (v, [which]) => { const u = state.su, i = +which, j = u.langs.indexOf(v); if (j < 0 || j === i) return; [u.langs[i], u.langs[j]] = [u.langs[j], u.langs[i]]; suOpen(); };
function suLangsDone() {      // the languages choose the Bibles offered: the first in each language, top and below
  const u = state.su, pick = l => SU_BIBLES[l] ? SU_BIBLES[l][0][0] : null;
  if (!u.top || (suBible(u.top) || {}).lang !== u.langs[0]) u.top = pick(u.langs[0]);
  if (u.langs[1] && (!u.below || (suBible(u.below) || {}).lang !== u.langs[1])) u.below = pick(u.langs[1]);
  if (!u.langs[1]) u.below = null;
}
C.suSongs = (v, [l]) => { state.su.songs[l] = v; suOpen(); };
C.suLinked = v => { DOC.settings.catLinked = v; };
A.suOthers = () => { state.su.others = true; suOpen(); };
A.suLook = () => { state.su.looked = true; suOpen(); };
A.suRemove = ([l]) => catRemove(l, suOpen);
// Taking a language's songs off asks first: Undo can't download them again.
function catRemove(l, then) { const L = CATALOG.langs[l];
  openSheet({ title: `Remove the ${L.en} songs?`, body: html`<p class="ui-p">${fmtN(L.songs)} songs from ${CATALOG.name} come off this computer and out of find. They can be downloaded again. Songs already in your library stay.</p>`,
    buttons: [{ t: 'Cancel', f: () => { then(); return false; } }, { t: 'Remove', primary: true, f: () => { delete catHave()[l]; toast(`${L.en} songs from ${CATALOG.name} are off this computer`); then(); return false; } }] }); }
A.suRetry = () => { const st = catState(); st.offline = false; const again = Object.keys(st.fail); st.fail = {}; again.forEach(catDownload); suOpen(); };
C.suBible = (v, [which]) => { state.su[which] = v || null; suOpen(); };
A.suScreen = ([which, name]) => { state.su[which] = name; suOpen(); };
function suTest() { const d = decks().find(x => x.song && x.seq && x.seq.length); if (d) go(d.key, 0); toast(state.su.aud === 'Not now' ? 'A test slide is up, but no screen is chosen to show it' : `A test slide is on ${state.su.aud}. Esc clears it`); }
function suFinish() {
  const u = state.su;
  mutate(() => { DOC.settings.langs = [...u.langs]; DOC.settings.setupDone = true;
    const rows = B().rows; if (!u.skipped[2]) { if (u.top && BIBLE_DATA.bibles[u.top]) rows[0].bible = u.top; if (u.below && BIBLE_DATA.bibles[u.below]) { if (rows[1]) rows[1].bible = u.below; else rows.push({ bible: u.below, verse: 'verse2', ref: 'ref2' }); } else if (!u.below) rows.splice(1); } });      // the Bible page's translations line: skipped, it stays as it was
  if (Object.keys(catState().dl).length) toast(`Downloading songs from ${CATALOG.name}. You can start; they become findable as they arrive.`);
}
// A download: carries on behind the setup, and after it, with its progress; a notice says when the songs can be found.
function catDownload(l) {
  const st = catState(); if (st.dl[l] || catHave()[l]) return;
  st.dl[l] = { pct: 0 }; const step = Math.max(4, Math.round(40 / Math.max(1, CATALOG.langs[l].songs / 3000)));
  const tick = () => { const d = st.dl[l]; if (!d) return; d.pct = Math.min(100, d.pct + step);
    if (d.pct >= 100) { delete st.dl[l]; st.have[l] = { at: Date.now() }; toast(`${fmtN(CATALOG.langs[l].songs)} ${CATALOG.langs[l].en} songs from ${CATALOG.name} can be found now`); if (state.mode === 'show' && !state.sheet) renderShowLeft(); }
    const p = $('suProgress'); if (p) put(p, suProgress()); const pct = document.querySelector('.su-pct'); if (pct && state.su) { const dl = Object.keys(st.dl); pct.textContent = dl.length ? Math.round(dl.reduce((n, x) => n + st.dl[x].pct, 0) / dl.length) + '%' : 'done'; }
    if (st.dl[l]) setTimeout(tick, 350); };
  setTimeout(tick, 350);
}

// Settings › Integrations: the catalog's languages, updates and credit. Nothing names it on the audience's screens unless chosen.
function catCard(card) {
  // The songbooks in three groups, as the app has them (24 November, 0a0212e): those on this computer, those recommended
  // (the church's languages; the app recommends the operator's language until it asks which languages the church shows),
  // then the other languages
  const st = catState(), langs = CAT_LANGS.filter(l => CATALOG.langs[l].songs), have = catHave();
  const church = DOC.settings.langs && DOC.settings.langs.length ? DOC.settings.langs : ['en'];
  const row = l => { const L = CATALOG.langs[l], got = have[l], dl = st.dl[l];
    return stLine(L.name, got ? html`<button class="btn" data-a="say:Checking ${L.en} songs for changes">Update Now</button><button class="btn" data-a="catDrop:${l}">Remove</button>` : dl ? html`<span class="sub">Downloading · ${dl.pct}%</span>` : html`<button class="btn" data-a="catGet:${l}">Download</button>`,
      got ? `${fmtN(L.songs)} songs · checked ${suAgo(got.at)}` : `${fmtN(L.songs)} songs · ${L.mb} to download · ${L.disk} on this computer`); };
  const group = (title, ls) => ls.length ? html`<div class="st-group"><span class="cap">${title}</span>${ls.map(row)}</div>` : '';
  return card(CATALOG.name, 'Free songbooks, one for each language', [
    group('On this computer', langs.filter(l => have[l])), group('Recommended', langs.filter(l => !have[l] && church.includes(l))), group('Other languages', langs.filter(l => !have[l] && !church.includes(l))),
    stLine('Second language', onoff(DOC.settings.catLinked !== false, 'catLinkedSet'), 'A song’s version in your second language comes along below it, group by group, when it has one'),
    stLine('Name it in the copyright line', onoff(!!DOC.settings.catCredit, 'catCreditSet'), `Adds “Songs: ${CATALOG.name}” where the copyright line shows, on the audience’s screens. Off unless you choose it`),
    html`<p class="lede" style="margin:10px 0 12px">Songs you haven’t changed are brought up to date about once a month, never while something is on screen. A song you changed is never overwritten: its details say when a newer version is there.</p>`],
    html`<button class="btn sm" data-a="catSite">${CATALOG.site} ↗</button>`);
}
A.catGet = ([l]) => { catDownload(l); render(); };
A.catDrop = ([l]) => catRemove(l, () => { closeSheet(); render(); });
A.catLinkedSet = () => mutate(() => { DOC.settings.catLinked = DOC.settings.catLinked === false; });
A.catCreditSet = () => mutate(() => { DOC.settings.catCredit = !DOC.settings.catCredit; });

/* ═════════ drag.js ═════════ */
// Dragging: slides and playlist items into a new order, songs onto a playlist or library, pictures and videos onto slides, items and placeholders.
'use strict';

let dragging = null;
const dragOf = el => { const d = el && el.closest && el.closest('[data-drag]'); if (!d) return null; const n = d.dataset.drag.indexOf(':'); return { kind: d.dataset.drag.slice(0, n), args: d.dataset.drag.slice(n + 1).split(','), el: d }; };
// Which half of the thing the pointer is over: dropping lands after it in the far half, before it in the near half.
const farHalf = (el, e, across) => { const r = el.getBoundingClientRect(); return across ? e.clientX > r.left + r.width / 2 : e.clientY > r.top + r.height / 2; };
function dropFor(el, e = {}) {      // what dropping here would do, or null
  if (!dragging || !el.closest) return null;
  const k = dragging.kind, over = dragOf(el), src = el.closest('[data-a^="src:"]'), slot = el.closest('[data-a^="fillSlot:"]'), bar = el.closest('.grp-h[id^="deck-"]');
  if (k === 'slide' && over && over.kind === 'slide' && over.args[0] === dragging.args[0] && over.el !== dragging.el) { const sg = deckByKey(over.args[0]).song;
    if (sg && arrOf(sg) && sg.slides[+over.args[1]].group !== sg.slides[+dragging.args[1]].group) return null; }      // under an arrangement the groups stand where the arrangement puts them: a slide moves only among its own group
  if (k === 'slide' && over && over.kind === 'slide' && over.args[0] === dragging.args[0] && over.el !== dragging.el) return { el: over.el, what: 'moveSlide', to: +over.args[1], after: farHalf(over.el, e, true), across: true };
  if (k === 'item' && over && over.kind === 'item' && over.el !== dragging.el) return { el: over.el, what: 'moveItem', to: over.args[0], after: farHalf(over.el, e) };
  if (k === 'find') {      // from the find list: only into the list shown (the owner, 23 November: to put it elsewhere, open that first). Into the open
    // playlist at the line or onto a placeholder; into the library listed, songs only. The chooser doesn't come up.
    const song = dragging.args[0] !== 'passage', slotRow = over && over.kind === 'item' && (byId(playlist().items, over.args[0]) || {}).kind === 'placeholder';
    if (over && over.kind === 'item' && state.src.type === 'pl') return slotRow && song ? { el: over.el, what: 'findInSlot', to: over.args[0], into: true } : { el: over.el, what: 'findToItem', to: over.args[0], after: farHalf(over.el, e) };
    if (song && state.src.type === 'lib' && state.src.id !== 'all' && el.closest('#left .items')) return { el: el.closest('#left .items'), what: 'findTo', type: 'lib', id: state.src.id, into: true };
    return null;
  }
  if (k === 'edtile' && over && over.kind === 'edtile' && over.args[0] === dragging.args[0] && over.el !== dragging.el) return { el: over.el, what: 'moveEdTile', to: over.args[1], after: farHalf(over.el, e, false) };
  if (k === 'media' && state.mediaView && over && over.kind === 'media' && over.el !== dragging.el && over.el.closest('.bin')) return { el: over.el, what: 'moveMedia', to: over.args[0], after: farHalf(over.el, e, true), across: true };
  if (k === 'edslide' && over && over.kind === 'edslide' && over.el !== dragging.el) return { el: over.el, what: 'moveEdSlide', to: +over.args[0], after: farHalf(over.el, e) };
  if (k === 'arr') {       // anywhere along the arrangement is a place to drop: the nearest piece decides, and the side of it the pointer is on
    const row = el.closest('.sh-arr'); if (!row || row.dataset.arr !== dragging.args[0]) return null;
    const x = e.clientX || 0, y = e.clientY || 0, chips = [...row.querySelectorAll('[data-drag^="arr:"]')]; let best = null, bd = Infinity;
    for (const c of chips) { const r = c.getBoundingClientRect(), dist = Math.abs(y - (r.top + r.height / 2)) * 4 + Math.abs(x - (r.left + r.width / 2)); if (dist < bd) { bd = dist; best = c; } }
    if (!best) return null;
    const r = best.getBoundingClientRect(), after = x > r.left + r.width / 2, i = chips.indexOf(best), me = chips.indexOf(dragging.el);
    if (best === dragging.el || (after && i + 1 === me) || (!after && i - 1 === me)) return { el: dragging.el, what: 'stay' };      // it would land where it already is
    return { el: best, what: 'moveArr', to: +best.dataset.drag.split(',')[1], after, across: true };
  }
  if (k === 'step' && over && over.kind === 'step' && over.args[0] === dragging.args[0] && over.el !== dragging.el) return { el: over.el, what: 'moveStep', to: +over.args[1], after: farHalf(over.el, e) };
  if (k === 'track' && over && over.kind === 'track' && over.args[0] === dragging.args[0] && over.el !== dragging.el) return { el: over.el, what: 'moveTrack', to: +over.args[1], after: farHalf(over.el, e) };
  if (k === 'song' && src) { const [type, id] = src.dataset.a.slice(4).split(','); if (type === 'pl' || (type === 'lib' && id !== 'all')) return { el: src, what: 'songTo', type, id }; }
  if (k === 'media') {
    if (over && over.kind === 'slide' && deckByKey(over.args[0]).song) return { el: over.el, what: 'mediaOnSlide', key: over.args[0], s: +over.args[1] };
    if (over && over.kind === 'item') return { el: over.el, what: 'mediaAfterItem', to: over.args[0] };
    if (slot) return { el: slot, what: 'mediaInSlot', id: slot.dataset.a.split(':')[1] };
    if (bar) { const d = deckByKey(bar.id.slice(5)); if (d && d.song) return { el: bar, what: 'mediaAsSlide', key: d.key }; }
  }
  return null;
}
const clearDropMarks = () => document.querySelectorAll('.dropping').forEach(x => x.classList.remove('dropping', 'drop-after', 'drop-across', 'drop-into'));
document.addEventListener('dragstart', e => {
  const d = dragOf(e.target); if (!d) return;
  d.x = e.clientX; d.y = e.clientY;
  dragging = d; d.el.classList.add('drag-src'); document.body.classList.add('is-dragging'); if (d.kind === 'song') document.body.classList.add('is-dragging-song'); if (d.kind === 'find') document.body.classList.add('is-dragging-find');      // the find list stays open; the chooser shows for songs from the song list only
  try { e.dataTransfer.setData('text/plain', d.el.dataset.drag); e.dataTransfer.effectAllowed = 'copyMove'; } catch (_) { /* a scripted drag has no data transfer */ }
});
document.addEventListener('dragover', e => { if (dragging) { const o = dragOf(e.target); dragging.home = !!o && o.el === dragging.el; } const t = dropFor(e.target, e); clearDropMarks(); if (!t) return; e.preventDefault(); if (t.what === 'stay') return; t.el.classList.add('dropping'); if ('after' in t) { t.el.classList.toggle('drop-after', t.after); t.el.classList.toggle('drop-across', !!t.across); } else t.el.classList.add('drop-into'); });
document.addEventListener('dragend', e => {
  const d = dragging; dragging = null; clearDropMarks(); if (d) d.el.classList.remove('drag-src'); document.body.classList.remove('is-dragging', 'is-dragging-song', 'is-dragging-find'); pointerHeld = false; flushRender();
  if (d && Math.hypot(e.clientX - d.x, e.clientY - d.y) > 10) return;      // it really was dragged, then brought back: that is a change of mind, not a click
  if (d && d.home && d.el.isConnected && d.el.dataset.a) act(d.el.dataset.a, d.el, e);      // a sloppy click: it ended on the thing it started on
});
document.addEventListener('drop', e => {
  const t = dropFor(e.target, e), d = dragging; if (!t) return; if (t.what === 'stay') return e.preventDefault();      // not a place to drop: the drag's end decides what happens
  dragging = null; clearDropMarks(); if (d) d.el.classList.remove('drag-src'); document.body.classList.remove('is-dragging', 'is-dragging-song', 'is-dragging-find'); pointerHeld = false; e.preventDefault();
  mutate(() => {
    if (t.what === 'moveSlide') {       // the picked slides move together, in their order; the slide on screen stays on screen
      const key = d.args[0], from = +d.args[1], sl = deckByKey(key).song.slides, idx = targets(key, from), moving = idx.map(i => sl[i]), before = sl[t.to];
      if (moving.includes(before)) return;
      [...idx].reverse().forEach(i => sl.splice(i, 1)); sl.splice(sl.indexOf(before) + (t.after ? 1 : 0), 0, ...moving); state.picked = { deck: null, set: [] };
    }
    if (t.what === 'moveItem') {      // the picked items move together, in their order, to the one line
      const items = playlist().items, ids = itemSel().includes(d.args[0]) ? itemSel() : [d.args[0]], moving = items.filter(i => ids.includes(i.id)), before = byId(items, t.to);
      if (moving.includes(before)) return; moving.forEach(it => items.splice(items.indexOf(it), 1)); items.splice(items.indexOf(before) + (t.after ? 1 : 0), 0, ...moving); }
    if (t.what.startsWith('find')) {      // a slide found by its words brings its whole song; then the search ends and the item is opened, nothing live
      let [kind, ...a] = d.args; if (kind === 'cat') { const j = catJoin(a[0]); kind = 'song'; a = [j.id]; if (j.from.with) toast(`“${j.title}” is in your library now, with its ${CATALOG.langs[j.from.with].en} version below`); }      // from the catalog: it joins the library as it goes in
      const item = () => kind === 'song' ? { id: newId('it'), kind: 'song', song: a[0] } : { id: newId('it'), kind: 'passage', passage: { book: a[0], chapter: +a[1], from: +a[2], to: +a[3] } };
      const name = kind === 'song' ? songById(a[0]).title : `${a[0]} ${a[1]}:${a[2]}${a[3] !== a[2] ? '-' + a[3] : ''}`;
      if (t.what === 'findToItem') { const items = playlist().items, it = item(); items.splice(items.indexOf(byId(items, t.to)) + (t.after ? 1 : 0), 0, it); state.openItem = it.id; state.itemPick = [it.id]; }
      if (t.what === 'findInSlot') { const it = byId(playlist().items, t.to); it.song = a[0]; state.openItem = it.id; }
      if (t.what === 'findTo' && t.type === 'pl') { const p = byId(DOC.playlists, t.id); p.items.push(item()); toast(`Added ${name} to ${p.name}.`); }
      if (t.what === 'findTo' && t.type === 'lib') { const l = byId(DOC.libraries, t.id); if (!l.songs.includes(a[0])) l.songs.push(a[0]); toast(`Added ${name} to ${l.name}.`); }
      $('find').value = ''; $('find').blur(); renderFind(); }
    if (t.what === 'moveEdTile') {      // templates (within the set shown), props, stage layouts and themes, as slides are
      const [kind, id] = d.args, th = edTarget().theme, L = { tpl: th && th.templates, prop: DOC.props, layout: DOC.stageLayouts, theme: DOC.themes }[kind];
      if (L) { const it = byId(L, id), before = byId(L, t.to); if (kind === 'theme' && before && before.builtIn && !t.after) return; if (it && before && it !== before) { L.splice(L.indexOf(it), 1); L.splice(L.indexOf(before) + (t.after ? 1 : 0), 0, it); } } }
    if (t.what === 'moveMedia') { const L = byId(DOC.mediaPlaylists, state.mediaView).items, from = L.indexOf(d.args[0]); let to = L.indexOf(t.to) + (t.after ? 1 : 0); if (from < to) to--; L.splice(to, 0, L.splice(from, 1)[0]); }      // the order shown is the order played
    if (t.what === 'moveEdSlide') { const tt = edTarget(), sl = tt.song.slides, it = sl[+d.args[0]], before = sl[t.to]; sl.splice(sl.indexOf(it), 1); sl.splice(sl.indexOf(before) + (t.after ? 1 : 0), 0, it); it.group = before.group; state.edit.target.k = sl.indexOf(it); }      // it takes the group of the slide it lands beside
    if (t.what === 'moveArr') arrMove(arrOf(songById(d.args[0])).groups, +d.args[1], t.to, t.after);
    if (t.what === 'moveStep') { const st = byId(DOC.macros, d.args[0]).steps, from = +d.args[1]; let to = t.to + (t.after ? 1 : 0); if (from < to) to--; st.splice(to, 0, st.splice(from, 1)[0]); }
    if (t.what === 'moveTrack') { const pl = byId(DOC.audio, d.args[0]), from = +d.args[1]; let to = t.to + (t.after ? 1 : 0); if (from < to) to--; trackMove(pl, from, to); }
    if (t.what === 'songTo') { const ids = (state.songPick.includes(d.args[0]) ? state.songPick : [d.args[0]]).map(id => id.startsWith('wla-') ? catJoin(id.slice(4)).id : id);      // a catalog song joins the library as it goes
      if (t.type === 'pl') { const p = byId(DOC.playlists, t.id); ids.forEach(id => p.items.push({ id: newId('it'), kind: 'song', song: id })); toast(ids.length > 1 ? `Added ${ids.length} songs to ${p.name}.` : `Added ${songById(ids[0]).title} to ${p.name}.`); }
      else { const l = byId(DOC.libraries, t.id); ids.forEach(id => { if (!l.songs.includes(id)) l.songs.push(id); }); toast(ids.length > 1 ? `Added ${ids.length} songs to ${l.name}.` : `Added ${songById(ids[0]).title} to ${l.name}.`); }
      state.songPick = []; }
    if (t.what === 'mediaOnSlide') targets(t.key, t.s).forEach(i => { deckByKey(t.key).song.slides[i].media = d.args[0]; });
    if (t.what === 'mediaAfterItem') { const items = playlist().items; items.splice(items.indexOf(byId(items, t.to)) + 1, 0, { id: newId('it'), kind: 'media', media: d.args[0] }); }
    if (t.what === 'mediaInSlot') { const it = byId(playlist().items, t.id); it.kind = 'media'; it.media = d.args[0]; delete it.song; }
    if (t.what === 'mediaAsSlide') { const m = mediaById(d.args[0]); deckByKey(t.key).song.slides.push(slide('', '', '', { media: m.id, label: m.name, objects: [] })); }
  });
});

/* ═════════ files.js ═════════ */
// Real files: song words, pictures, videos and audio chosen from this computer. They stay in the browser; nothing is uploaded.
'use strict';

function pickFiles(accept, done) {
  const inp = document.createElement('input'); inp.type = 'file'; inp.accept = accept; inp.multiple = true;
  inp.addEventListener('change', () => { if (inp.files.length) done([...inp.files]); });
  inp.click();
}
const natural = (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });       // Slide2 before Slide10

/* ───────── song words from a text, lyric or ChordPro file ───────── */
const SECTION_RE = /^(verse|chorus|refrain|bridge|pre-?\s?chorus|intro|outro|ending|tag|interlude|куплет|запев|припев|приспів|мост|бридж|вступление|окончание)\s*(\d*)\s*[:.]?$/i;
const SECTION_EN = { refrain: 'Chorus', 'куплет': 'Verse', 'запев': 'Verse', 'припев': 'Chorus', 'приспів': 'Chorus', 'мост': 'Bridge', 'бридж': 'Bridge', 'вступление': 'Intro', 'окончание': 'Ending' };
function importSongText(fileName, raw) {
  const info = { artist: '', author: '', publisher: '', year: '', ccli: '' }; let title = '';
  const text = raw.replace(/^﻿/, '').replace(/\r\n?/g, '\n'), chordpro = /\{\s*(title|t|soc|start_of_chorus|comment|c|start_of_verse|sov)\b/i.test(text);
  const lines = []; let tab = false;
  for (let ln of text.split('\n')) {
    if (chordpro) {
      if (/^\s*#/.test(ln)) continue;
      const d = /^\s*\{\s*([a-z_]+)\s*:?\s*(.*?)\s*\}\s*$/i.exec(ln);
      if (d) { const k = d[1].toLowerCase(), v = d[2];
        if (k === 'title' || k === 't') title = v; else if (k === 'artist' || k === 'subtitle' || k === 'st') info.artist = v; else if (k === 'composer' || k === 'lyricist') info.author = v; else if (k === 'copyright') info.publisher = v; else if (k === 'year') info.year = v;
        else if (k === 'comment' || k === 'c') lines.push('', `[${v}]`); else if (/^(soc|start_of_chorus)$/.test(k)) lines.push('', `[${v || 'Chorus'}]`); else if (/^(sov|start_of_verse)$/.test(k)) lines.push('', `[${v || 'Verse'}]`); else if (/^(sob|start_of_bridge)$/.test(k)) lines.push('', `[${v || 'Bridge'}]`);
        else if (/^(eoc|eov|eob|end_of_)/.test(k)) lines.push(''); else if (/^(sot|start_of_tab)$/.test(k)) tab = true; else if (/^(eot|end_of_tab)$/.test(k)) tab = false;
        continue; }
      if (tab) continue;
      ln = ln.replace(/\[[^\]]*\]/g, '').replace(/\s{2,}/g, ' ');       // the chords are taken out
    }
    lines.push(ln.trimEnd());
  }
  const body = [];
  for (const ln of lines) { const t = ln.trim(); let m;      // the footer song services add
    if ((m = /^CCLI Song\s*#\s*(\d+)/i.exec(t))) { info.ccli = m[1]; continue; }
    if (/^CCLI Licen[cs]e\s*#/i.test(t) || /^For use solely with/i.test(t)) continue;       // a church's own licence number is never kept
    if ((m = /^©\s*(\d{4})?\s*(.*)$/.exec(t))) { info.year = m[1] || info.year; info.publisher = m[2] || info.publisher; continue; }
    if (/ \| /.test(t) && !SECTION_RE.test(t)) { info.author = t.replace(/ \| /g, ', '); continue; }
    body.push(ln);
  }
  while (body.length && !body[0].trim()) body.shift();
  const isHead = t => SECTION_RE.test(t.trim()) || /^\[.+\]$/.test(t.trim());
  if (!title && body.length > 1 && !body[1].trim() && !isHead(body[0])) title = body.shift().trim();
  if (!title) title = fileName.replace(/\.[^.]+$/, '');
  const drafts = []; let group = 'Verse 1', cur = null;
  for (const ln of body) { const t = ln.trim(), sq = /^\[(.+)\]$/.exec(t), m = SECTION_RE.exec(sq ? sq[1] : t);
    if (m) { const base = SECTION_EN[m[1].toLowerCase()] || m[1][0].toUpperCase() + m[1].slice(1).toLowerCase().replace(/^pre.*/i, 'Pre-chorus'); group = (base + ' ' + m[2]).trim(); if (base === 'Verse' && !m[2]) group = 'Verse 1'; cur = null; continue; }
    if (sq) { group = sq[1].trim(); cur = null; continue; }
    if (!t) { cur = null; continue; }
    if (!cur) { cur = { group, lines: [] }; drafts.push(cur); }
    cur.lines.push(t);
  }
  const made = song(title, []); made.info = info; made.slides = [];
  for (const d of drafts) { const n = Math.ceil(d.lines.length / 4), size = Math.ceil(d.lines.length / n);       // more than four lines are shared out evenly
    for (let i = 0; i < d.lines.length; i += size) made.slides.push(slide(d.group, d.lines.slice(i, i + size).join('\n'), '')); }
  if (!made.slides.length) made.slides.push(slide('Verse 1', '', ''));
  return made;
}
function addImportedSongs(list) {
  const have = DOC.songs, fresh = list.filter(s => !have.some(h => h.title === s.title && songText(h) === songText(s))), same = list.filter(s => !fresh.includes(s));      // a song already here word for word isn't added twice
  if (fresh.length) mutate(() => { fresh.forEach(s => { DOC.songs.push(s); if (state.src.type === 'lib' && state.src.id !== 'all') byId(DOC.libraries, state.src.id).songs.push(s.id); });
    if (state.src.type !== 'lib') state.src = { type: 'lib', id: 'all' };
    state.openSong = fresh[0].id; state.mode = 'show'; });
  importReport('Songs', fresh.map(s => ({ name: s.title, note: `${s.slides.length} slide${s.slides.length === 1 ? '' : 's'} · ${[...new Set(s.slides.map(x => x.group))].length} groups`, a: `openImported:${s.id}` })), same.map(s => ({ name: s.title, note: 'Already here, word for word' })));
}
// What an import did: what came in, and what was passed over and why.
function importReport(what, came, skipped) {
  const row = x => html`<div class="ui-row"><span class="t">${x.name}</span><span class="sub">${x.note}</span>${x.a ? html`<button class="btn sm" data-a="${x.a}">Open</button>` : ''}</div>`;
  openSheet({ title: `${what}: ${came.length} came in${skipped.length ? `, ${skipped.length} passed over` : ''}`, wide: true,
    body: came.length || skipped.length ? html`${came.length ? html`<span class="cap">Came in</span><div class="ui-list">${came.map(row)}</div>` : ''}${skipped.length ? html`<span class="cap" style="margin-top:8px">Passed over</span><div class="ui-list">${skipped.map(row)}</div>` : ''}` : html`<p class="ui-p">Nothing to bring in.</p>`,
    note: came.length ? 'Undo takes the whole import back' : '', buttons: [{ t: 'Done', primary: true }] });
}
A.openImported = ([id]) => { closeSheet(); state.mode = 'show'; state.src = { type: 'lib', id: 'all' }; state.openSong = id; render(); };

/* ───────── pictures and videos ───────── */
function mediaFromFile(f) {
  const url = URL.createObjectURL(f), video = f.type.startsWith('video/');
  const m = { id: newId('m'), name: f.name, url, real: true, scaling: 'Scale to Fill', bg: video ? 'linear-gradient(135deg, #24242b, #101014)' : `url("${url}") center / cover no-repeat #000` };
  if (video) { m.video = true; m.loop = true; m.length = 60; const v = document.createElement('video'); v.preload = 'metadata'; v.src = url; v.onloadedmetadata = () => { const real = mediaById(m.id); if (real && isFinite(v.duration)) { real.length = Math.max(1, Math.round(v.duration)); render(); } }; }
  return m;
}
function addMediaFiles(files) { const made = files.filter(f => /^(image|video)\//.test(f.type)).map(mediaFromFile); if (!made.length) return refuse('Choose pictures or videos'); mutate(() => { DOC.media.push(...made); if (state.mediaView) byId(DOC.mediaPlaylists, state.mediaView).items.push(...made.map(m => m.id)); });  }
function addPictureSlides(files) {
  const pics = files.filter(f => f.type.startsWith('image/')).sort(natural); if (!pics.length) return refuse('Choose pictures. A presentation or a PDF is turned into pictures by the app.');
  mutate(() => { const s = song(pics.length === 1 ? pics[0].name.replace(/\.[^.]+$/, '') : 'Imported slides', []);
    s.slides = pics.map((f, n) => { const m = { ...mediaFromFile(f), deck: true }; m.bg = `url("${m.url}") center / contain no-repeat #fff`; DOC.media.push(m); return slide('', '', '', { label: `Page ${n + 1}`, objects: [obj('image', f.name, { media: m.id, frame: { x: 0, y: 0, w: 1920, h: 1080 } })] }); });
    DOC.songs.push(s); state.src = { type: 'lib', id: 'all' }; state.openSong = s.id; state.mode = 'show'; });
  importReport('Pictures', pics.map((p, n) => ({ name: p.name, note: `Slide ${n + 1}` })), files.filter(p => !p.type.startsWith('image/')).map(p => ({ name: p.name, note: 'Not a picture' })));
}

/* ───────── audio: a real file plays; the sample tracks are silent ───────── */
const audioEl = new Audio();
audioEl.addEventListener('ended', () => { const a = state.audio, pl = audioPl(); if (pl) audioEnded(a, pl); else a.playing = false; syncAudio(); render(); });
function syncAudio() {
  const a = state.audio, pl = audioPl(), tr = pl && pl.tracks[a.track], real = tr && tr.url;
  if (!real || !a.playing) { if (!audioEl.paused) audioEl.pause(); if (!real) audioEl.removeAttribute('src'); return; }
  if (audioEl.dataset.url !== tr.url) { audioEl.dataset.url = tr.url; audioEl.src = tr.url; audioEl.currentTime = a.pos || 0; }
  audioEl.volume = a.muted ? 0 : a.volume / 100;
  if (Math.abs(audioEl.currentTime - a.pos) > 1.5) audioEl.currentTime = a.pos;
  if (audioEl.paused) audioEl.play().catch(() => {});
}
function addAudioFiles(files) {
  const made = files.filter(f => f.type.startsWith('audio/')).map(f => ({ name: f.name.replace(/\.[^.]+$/, ''), url: URL.createObjectURL(f), length: 180 })); if (!made.length) return refuse('Choose audio files');
  made.forEach(t => { const el = new Audio(); el.preload = 'metadata'; el.src = t.url; el.onloadedmetadata = () => { const real = DOC.audio.flatMap(p => p.tracks).find(x => x.url === t.url); if (real && isFinite(el.duration)) { real.length = Math.round(el.duration); render(); } }; });
  mutate(() => { audioPl().tracks.push(...made); }); toast(`${made.length} track${made.length > 1 ? 's' : ''} added`);
}

/* ═════════ remote-words.js ═════════ */
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
   "This device is remembered once it signs in, until it is forgotten in Settings › Remote Control on the computer.": "Este dispositivo se recuerda en cuanto inicia sesión, hasta que lo olvides en Ajustes › Control remoto en el ordenador.",
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

/* ═════════ remote-lang.js ═════════ */
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

/* ═════════ remote.js ═════════ */
'use strict';
// The link to SoftPresenter's Remote Control (the app's docs/remote-api.md): one WebSocket; the snapshot and each change
// take the place of the sample data; the show's controls go to the app as its own commands, aimed at this page's open item.
// Served by the app, the page starts here (window.SP_REMOTE). In the prototype, ?remote=fixture plays the app's captured
// samples through a stand-in socket, so the checks can run it without an app.
//
// Running the show works from here. Changing songs, playlists or settings is done at the computer for now: the app's Undo
// doesn't cover changes from a page yet (step 2 of the API), and this page never changes the app's data behind its back.

const REMOTE_MODE = window.SP_REMOTE === true ? 'app' : window.SP_REMOTE === 'fixture' || new URLSearchParams(location.search).get('remote') === 'fixture' ? 'fixture' : null;
const REMOTE = { queue: [], lastStep: 0, libs: {}, on: false, mode: null, socket: null, epoch: null, version: 0, parts: {}, you: null, offset: 0, nextId: 1, waiting: new Map(), retry: 500,
  preview: null, lastBeat: 0, watch: 0, status: 'off', sent: [], slides: new Map(), loading: new Set(), lists: {}, library: null, refusedEdit: false };
const AT_THE_COMPUTER = 'Changing songs, playlists and settings is done at the computer for now. Running the show works from here.';
const serverNow = () => Date.now() + REMOTE.offset;
// The app's values are checked where they come in: ids are plain words (a Uuid, a hex rev), colours are #rrggbb. Anything
// else is dropped, so nothing the app sends can change what an action's arguments are, or what a style does.
const ID = /^[\w-]{1,64}$/, okId = x => typeof x === 'string' && ID.test(x), okColour = c => /^#[0-9a-f]{6}$/i.test(String(c)) ? c : '#808080';
const ANSWERS = new Set(['outside:reload', 'outside:keep']);
// An id from a name, with nothing in it an action could split on (a name may hold a comma or a colon).
const nameId = (p, name) => p + [...String(name)].reduce((h, c) => (Math.imul(h, 31) + c.codePointAt(0)) >>> 0, 2166136261).toString(36);
const REASON = { bible: 'Looking up the Bible comes to the remote page with the next step. Use the Bible page at the computer for now.', edit: 'Changing songs, playlists and settings is done at the computer for now. Running the show works from here.', settings: 'Settings are changed at the computer.' };
// The sample's names, kept for the checks: none may show once the app's state has come.
const SAMPLE_NAMES = [...DOC.songs.map(x => x.title), ...DOC.playlists.map(x => x.name), ...DOC.libraries.map(x => x.name), ...DOC.media.map(x => x.name), ...DOC.timers.map(x => x.name), ...DOC.messages.map(x => x.name),
  ...DOC.props.map(x => x.name), ...DOC.macros.map(x => x.name), ...DOC.screens.map(x => x.name), ...DOC.stageLayouts.map(x => x.name), ...((DOC.voxlive || {}).screens || []).map(x => x.name)];
const SCREEN_LIKE = clone(DOC.screens.find(x => x.kind === 'Audience') || DOC.screens[0]);      // the fields a screen needs here, filled from the app's list

/* ───────── the socket ───────── */
function remoteStart(mode = REMOTE_MODE) {
  remoteSliders();
  REMOTE.mode = mode; REMOTE.on = true; REMOTE.status = 'connecting';
  emptyDoc(); render(); remoteConnect();      // only reads are sent until someone presses something
  clearInterval(REMOTE.watch); REMOTE.watch = setInterval(() => {      // no beat for 30 seconds: the connection is dead, though nobody said so
    if (REMOTE.socket && REMOTE.status === 'on' && Date.now() - REMOTE.lastBeat > 30000) REMOTE.socket.close();
  }, 5000);
}
function remoteConnect() {
  const ws = REMOTE.mode === 'fixture' ? new FixtureSocket() : new WebSocket('wss://' + location.host + '/api/ws');
  REMOTE.socket = ws; ws.binaryType = 'blob';
  ws.addEventListener('open', () => {
    REMOTE.retry = 500; REMOTE.lastBeat = Date.now();
    const hello = { type: 'hello', preview: true };
    if (REMOTE.epoch) hello.resume = { epoch: REMOTE.epoch, version: REMOTE.version };
    ws.send(JSON.stringify(hello));
  });
  ws.addEventListener('message', e => {
    if (typeof e.data !== 'string') return remotePicture(e.data);
    let m; try { m = JSON.parse(e.data); } catch (_) { return; }
    remoteTake(m);
  });
  ws.addEventListener('close', e => {
    if (REMOTE.socket !== ws) return;
    for (const [, done] of REMOTE.waiting) done({ ok: false, text: 'Not connected.' });
    REMOTE.waiting.clear(); REMOTE.socket = null;
    if (e.code === 4401) { REMOTE.status = 'off'; return location.replace('/login'); }      // signed out: the device was forgotten, or the password changed
    if (e.code === 4000) { REMOTE.status = 'stopped'; renderTop(); return trouble('Disconnected at the computer. Click Connect in the top bar to connect again.'); }
    REMOTE.status = 'lost'; renderTop();
    setTimeout(() => { if (REMOTE.on && !REMOTE.socket && REMOTE.status !== 'stopped') remoteConnect(); }, REMOTE.retry);
    REMOTE.retry = Math.min(REMOTE.retry * 2, 8000);
  });
}
function remoteTake(m) {
  if (typeof m.now === 'number') REMOTE.offset = m.now - Date.now();
  if (m.type === 'snapshot') { REMOTE.epoch = m.epoch; REMOTE.version = m.version; REMOTE.parts = m.parts || {}; REMOTE.you = m.you || null; REMOTE.lastBeat = Date.now(); REMOTE.status = 'on'; remoteApply(true); }
  else if (m.type === 'change') { REMOTE.version = m.version; Object.assign(REMOTE.parts, m.parts || {}); REMOTE.lastBeat = Date.now(); REMOTE.status = 'on'; remoteApply(false, Object.keys(m.parts || {})); }
  else if (m.type === 'reply') { const done = REMOTE.waiting.get(m.id); REMOTE.waiting.delete(m.id); if (done) done(m.reply || { ok: false }); }
  else if (m.type === 'beat') REMOTE.lastBeat = Date.now();
  else if (m.type === 'error') trouble(m.text || 'The app couldn’t read what this page sent.');
}
// The live preview: the first byte says what the picture is (1: what the audience sees), a JPEG after it.
function remotePicture(blob) {
  blob.slice(0, 1).arrayBuffer().then(head => {
    if (new Uint8Array(head)[0] !== 1) return;
    const url = URL.createObjectURL(blob.slice(1, blob.size, 'image/jpeg')), old = REMOTE.preview;
    REMOTE.preview = url; remotePaintPreview(); if (old) setTimeout(() => URL.revokeObjectURL(old), 1000);
  });
}
// A command, as `sp` would type it. A refusal comes back in the app's own words, shown as the cue under what was clicked.
// An ok reply shows nothing, an end reached (`"at_end": true`) among them: the owner wants nothing said there.
function remoteSend(args, quiet) {
  REMOTE.sent.push(args);
  return new Promise(resolve => {
    const ws = REMOTE.socket;
    if (!ws || ws.readyState !== 1) return resolve({ ok: false, text: 'Not connected to SoftPresenter.' });
    if (REMOTE.waiting.size >= 4) return resolve({ ok: false, text: 'Waiting for SoftPresenter to answer. Try again in a moment.', busy: true });      // the server lets four wait
    const id = REMOTE.nextId++; REMOTE.waiting.set(id, resolve);
    ws.send(JSON.stringify({ type: 'command', id, args: args.map(String) }));
  }).then(r => { if (!r.ok && !quiet) refuse(r.text || 'SoftPresenter said no.'); return r; });
}

/* ───────── the app's state, as this page's ───────── */
function emptyDoc() {      // never through clear(): only reads go to the app until someone presses something
  Object.assign(state, { live: null, on: null, mediaOn: null, mediaPl: null, propsOn: [], message: null, ann: null, vox: null, cue: null, heldDeck: null, heldBible: null }); undoStack = []; redoStack = [];
  Object.assign(DOC, { songs: [], libraries: [], playlists: [{ id: 'rpl:', name: 'Connecting…', items: [] }], media: [], mediaPlaylists: [], timers: [], messages: [], props: [], macros: [], audio: [], versions: [], groups: [], screens: [] });      // nothing of the sample shows on the real page
  state.src = { type: 'pl', id: 'rpl:' }; state.openItem = null; state.mode = 'show'; state.mediaView = null; state.picked = { deck: null, set: [] }; state.songPick = []; state.mediaPick = []; state.itemPick = [];
  NOTES.length = 0;
}
const TEST_PATTERNS = { ColorBars: 'bars', Grid: 'grid', White: 'white' };
const wordsSlideFrom = (s, img) => ({ id: s.id || newId('rs'), group: s.group || '', label: s.label || '', notes: '', enabled: s.enabled !== false, hotkey: s.hot_key || '', advance: 0, transition: null, template: null, actions: [], media: null, img: img || s.thumb || null,
  objects: [obj('text', 'Words', { role: 'main', text: String(s.words || '').split(' / ').join('\n'), frame: { x: 120, y: 140, w: 1680, h: 800 }, style: { size: 80 } })] });
function songObject(key, title, more) {
  const cached = REMOTE.slides.get(key);
  return { id: key, title, info: { artist: '', author: '', publisher: '', year: '', ccli: '' }, notes: '', theme: null, transition: null, loop: false, arrangements: [], arrangement: null,
    slides: cached ? cached.slides : [], loaded: !!cached, ...more };
}
function remoteApply(all, changed = []) {
  const P = REMOTE.parts, has = k => all || changed.includes(k);
  // The app's language and its way of writing numbers (parts.app): the page's words follow it, live (remote-lang.js)
  if (has('app') && P.app && typeof UI === 'object') UI.set(P.app.language, P.app.format);
  if (has('bin')) {
    DOC.media = ((P.bin || {}).items || []).filter(b => okId(b.id)).map(b => ({ id: b.id, num: b.number, name: b.name, bg: '#151515', poster: b.poster || null, video: b.kind === 'video', length: 0, loop: !!b.loop, scaling: b.fit === 'fit' ? 'Scale to Fit' : 'Scale to Fill' }));
    DOC.mediaPlaylists = ((P.bin || {}).playlists || []).filter(p => okId(p.id)).map(p => ({ id: p.id, name: p.name, items: [], follow: null }));
  }
  if (has('timers')) DOC.timers = (P.timers || []).filter(t => okId(t.id)).map(t => { const k = t.kind || {};
    return { id: t.id, name: t.name, kind: k.type === 'countdown' ? 'down' : k.type === 'countdown_to' ? 'to' : 'up', start: k.type === 'countdown' ? k.seconds || 0 : k.type === 'elapsed' ? k.start || 0 : 0,
      at: k.type === 'countdown_to' ? `${two(k.hour)}:${two(k.minute)}` : '10:30', left: 0, run: !!t.running, words: t.words, app: t }; });
  if (has('messages')) DOC.messages = (P.messages || []).map(m => ({ id: nameId('rm', m.name), name: m.name, text: m.text || '', leaves: m.leaves_after || 0, look: 'Band', showing: !!m.showing }));
  if (has('props')) DOC.props = (P.props || []).map(p => ({ id: nameId('rp', p.name), name: p.name, clears: p.clears_after || 0, objects: [], showing: !!p.showing }));
  if (has('macros')) DOC.macros = (P.macros || []).map((m, n) => ({ id: nameId('rc', m.name), name: m.name, color: SWATCHES[2 + n % 6], atStart: !!m.startup, steps: [], appSteps: m.steps || [] }));
  if (has('versions')) DOC.versions = ((P.versions || {}).versions || []).filter(v => okId(v.id)).map(v => ({ id: v.id, name: v.name }));
  if (has('screens')) DOC.screens = ((P.screens || {}).screens || []).map((x, n) => ({ ...clone(SCREEN_LIKE), id: 'rs' + n, name: x.name, kind: x.kind === 'stage' ? 'Stage' : 'Audience', on: x.shown !== false, doing: x.doing || '', tone: x.tone }));
  if (all) remoteSend(['groups'], true).then(r => { if (r.ok && Array.isArray(r.data)) { DOC.groups = r.data.map(g => ({ name: g.group, color: okColour(g.color), key: g.key || '', names: g.names || [] })); render(); } });      // the groups' colours, for the dots under the slides
  if (has('library')) { const rev = (P.library || {}).rev;      // the library changed: the lists open here are read again
    if (REMOTE.library && REMOTE.library.rev !== rev) remoteFetchSongs();
    for (const name of Object.keys(REMOTE.libs)) if (REMOTE.libs[name].rev !== rev) remoteFetchLibrary(name); }
  if (has('playlist') || has('playlists') || has('bin') || has('library')) remotePlaylists();
  if (has('notices')) remoteNotices();
  remoteLive();
  remoteTimes();
  fixState(); render();
}
// The playlists: the one in front with its items; the others by name (their items come when asked for).
function remotePlaylists() {
  const P = REMOTE.parts, front = P.playlist || { items: [] }, songs = new Map();
  const was = (() => { const pl = DOC.playlists.find(p => p.front), it = pl && pl.items.find(i => i.id === state.openItem); if (!it) return null;
    const so = it.song && byId(DOC.songs, it.song); return { kind: it.kind, appId: so && so.appId, title: so ? so.title : it.name || '', rev: pl.rev }; })();
  const item = i => {
    const uid = okId(i.uid) ? i.uid : null, id = uid ? 'ru' + uid : 'ri' + (Number(i.item) | 0), base = { id, num: Number(i.item) | 0, uid };      // an item's own id, kept across moves
    if (i.kind === 'header') return { ...base, kind: 'header', name: i.title };
    if (i.kind === 'placeholder') return { ...base, kind: 'placeholder', name: i.title, song: null };
    if (i.kind === 'media') { const m = DOC.media.find(x => x.num === i.number); return { ...base, kind: 'media', media: m ? m.id : 'gone' }; }
    if (i.kind === 'media_playlist') { const mp = DOC.mediaPlaylists.find(x => x.name === i.title); return { ...base, kind: 'mediaPlaylist', mp: mp ? mp.id : 'gone' }; }
    if (i.missing) return { ...base, kind: 'song', song: 'gone', title: i.title };
    // a song or a passage: its slides come with `slides item N`, kept under the item's number and the playlist's rev
    const lib = (P.library || {}).rev, key = uid ? `is${uid}r${okId(lib) ? lib : 'x'}` : `is${okId(front.rev) ? front.rev : 'x'}n${Number(i.item) | 0}`;      // read again when the library changes
    songs.set(key, songObject(key, i.title, { appId: okId(i.id) ? i.id : null, num: i.number || null, item: i.item, uid, passage: i.kind === 'passage', notInLibrary: true }));
    return { ...base, kind: 'song', song: key };
  };
  const frontItems = (front.items || []).map(item);
  // A built-in playlist shows its label in the app's language ("Culto") while its stored name ("Service") keeps its id the
  // same through a language switch; commands name it by its number (the app refuses the stored name while it shows another)
  const shown = x => (typeof x.label === 'string' && x.label) || x.name;
  DOC.playlists = (P.playlists || []).map(p => ({ id: nameId('rpl', p.name), name: shown(p), num: p.number, rev: p.rev, front: !!p.front, items: p.front ? frontItems : [] }));
  if (!DOC.playlists.length) DOC.playlists = [{ id: nameId('rpl', front.name || ''), name: shown(front) || 'Playlist', front: true, items: frontItems }];
  const libSongs = remoteLibrarySongs();
  DOC.songs = [...songs.values(), ...libSongs];
  DOC.libraries = ((P.library || {}).libraries || []).map(l => ({ id: nameId('rl', l.name), name: l.name, songs: REMOTE.libs[l.name] ? REMOTE.libs[l.name].songs.map(x => libKey(x.id)) : [] }));
  const f = DOC.playlists.find(p => p.front) || DOC.playlists[0];
  if (was && was.rev !== front.rev) {      // reordered, added to or taken from elsewhere: the open item is found again by what it is
    const same = it => { const so = it.song && songs.get(it.song); return it.kind === was.kind && (so ? (was.appId ? so.appId === was.appId : so.title === was.title) : (it.name || '') === was.title); };
    const now = frontItems.find(i => i.id === state.openItem); if (!now || !same(now)) { const found = frontItems.find(same); state.openItem = found ? found.id : null; }
    state.itemPick = []; state.cue = null;
  }
  const onAll = state.src.type === 'lib' && (state.src.id === 'all' || DOC.libraries.some(l => l.id === state.src.id));      // All Songs or a library stays open; anything else goes back to the playlist in front
  if (!onAll && (state.src.type !== 'pl' || !byId(DOC.playlists, state.src.id) || !byId(DOC.playlists, state.src.id).front)) state.src = { type: 'pl', id: f.id };
  if (!f.items.some(i => i.id === state.openItem)) state.openItem = (f.items.find(i => i.kind === 'song') || f.items[0] || {}).id || null;
}
function remoteNotices() {
  const list = REMOTE.parts.notices || [];
  NOTES.length = 0;
  for (const n of list) if (n.kind !== 'ask' && Number.isSafeInteger(n.id)) NOTES.push({ id: n.id, kind: n.kind, text: n.text, time: new Date((n.time || 0) * 1000), seen: !!n.seen, action: null });
}
// questions from the app, with its own answers as buttons
// A question the page may not answer shows without its buttons, saying where it is answered (a device is let onto the screens
// only at the computer: the app refuses mirror:allow from a page)
const remoteAsks = () => (REMOTE.parts.notices || []).filter(n => n.kind === 'ask' && Number.isSafeInteger(n.id)).map(n => { const acts = n.actions || [], ok = acts.filter(a => ANSWERS.has(a.id));
  const atComputer = !ok.length && acts.some(a => /^mirror:/.test(String(a.id)));      // its own words, so they can be put in the app's language
  return { kind: 'ask', text: n.text, html: atComputer ? html`${n.text} <span class="sub">Letting a phone, iPad or Mac show its screen is decided at the computer, not from the remote page.</span>` : null, actions: ok.map(a => [a.label, `remoteAnswer:${n.id},${a.id}`]) }; });
A.remoteAnswer = ([id, action]) => remoteSend(['notices', 'answer', id, action]);
// What is on screen, and the switches beside it.
function remoteLive() {
  const L = REMOTE.parts.live || {}, on = L.live;
  state.audience = L.audience_on !== false; state.stage = L.stage_on !== false; state.identify = !!L.identify; DOC.settings.continuous = !!L.continuous;
  state.test = TEST_PATTERNS[L.test_pattern] || 'off';
  state.stageMsg = L.stage_message && L.stage_message.on ? L.stage_message.text || '' : '';
  const V = (REMOTE.parts.versions || {}).on; state.version = V && V.id ? { id: V.id, keep: !!V.keep, from: null } : null;
  if (!on) { state.on = null; state.live = null; }
  else {
    const all = decks();
    const deck = (Number.isInteger(L.live_item) && all.find(d => d.item && d.item.num === L.live_item)) || (on.id && all.find(d => d.song && d.song.appId === on.id)) || null;
    const s = (on.slide || 1) - 1, song = deck && deck.song, sl = song && song.slides[s];
    state.live = deck ? { deck: deck.key, q: deck.seq ? deck.seq.indexOf(s) : s, s } : null;
    const was = state.on;
    state.on = { slide: sl ? clone(sl) : wordsSlideFrom({ words: on.words, group: '' }, L.thumb), songId: song ? song.id : null, kind: on.kind === 'passage' ? 'bible' : 'songs', raw: true, bg: '#000000',
      title: on.song || '', group: sl ? sl.group : '', n: on.slide, total: on.slides, tr: DOC.settings.transition, stamp: was && was.title === on.song && was.n === on.slide ? was.stamp : Date.now() };
    state.on.slide.img = L.thumb || state.on.slide.img;
  }
  const media = REMOTE.parts.media || {};
  state.mediaOn = media.showing ? media.showing.id : null;
  state.message = L.message ? ((DOC.messages.find(m => m.name === L.message) || {}).id || null) : null;
  state.propsOn = (L.props || []).map(n => (DOC.props.find(p => p.name === n) || {}).id).filter(Boolean);
  remoteAudio();
  state.ann = null; state.advanceLeft = 0; state.build = L.build || 0;
}
// Nothing that ticks is sent: times are worked out from when things started, on the app's clock.
function remoteTimes() {
  const now = serverNow();
  for (const t of DOC.timers) {
    const a = t.app || {}, k = a.kind || {}, counted = (a.counted_ms || 0) + (a.started_at ? now - a.started_at : 0);
    if (k.type === 'countdown') t.left = Math.max(a.overrun ? -Infinity : 0, Math.round(((k.seconds || 0) * 1000 - counted) / 1000));
    else if (k.type === 'elapsed') t.left = Math.round(Math.min((k.start || 0) * 1000 + counted, k.end == null ? Infinity : k.end * 1000) / 1000);
    else if (k.type === 'countdown_to') t.left = secondsTo(t.at);
    t.run = !!a.running;
  }
  const v = (REMOTE.parts.media || {}).video;
  if (v) { state.video.playing = !!v.playing; state.video.pos = Math.floor((v.started_at ? now - v.started_at : v.position_ms || 0) / 1000); state.video.loop = !!v.loop; state.video.mute = !!v.mute; }
  const au = REMOTE.parts.audio || {};
  state.audio.pos = Math.floor((au.started_at ? now - au.started_at : au.position_ms || 0) / 1000);
}
function remoteTick() { remoteTimes(); remoteAudioPlace(); paintAudio(); paintClock(); }      // in place, as the sample's clock does: a press is never lost to a redraw

// The audio player (the app since 9bc1cc6). The status names the playing track, where it is (from when it started, on the
// app's clock, or where it was paused) and the switches; the tracks' names come from `audio`, asked for when the track isn't
// among those known. Only the playing track's length is sent, so the others show none.
function remoteAudio() {
  const a = REMOTE.parts.audio || {}, names = REMOTE.tracks || [], at = names.indexOf(a.track);
  if (a.track && at < 0 && !REMOTE.tracksAsked) { REMOTE.tracksAsked = true;
    remoteSend(['audio'], true).then(r => { REMOTE.tracksAsked = false; if (r.ok && Array.isArray(r.data)) { REMOTE.tracks = r.data.filter(x => typeof x === 'string'); remoteAudio(); render(); } }); }
  const list = names.length ? names : a.track ? [a.track] : [];
  DOC.audio = [{ id: 'app', name: 'Audio', tracks: list.map(n => ({ name: n, length: n === a.track ? Math.round((a.length_ms || 0) / 1000) : 0 })) }];
  Object.assign(state.audio, { pl: 'app', track: Math.max(0, list.indexOf(a.track)), playing: !!a.playing && !a.paused, muted: !!a.mute, volume: a.volume == null ? state.audio.volume : a.volume,
    shuffle: !!a.shuffle, through: !!a.through, loop: !!a.loop, crossfade: a.crossfade || 0 });
  remoteAudioPlace();
}
function remoteAudioPlace() { const a = REMOTE.parts.audio || {}, len = Math.round((a.length_ms || 0) / 1000);
  state.audio.pos = Math.max(0, Math.min(len, Math.floor((a.started_at ? serverNow() - a.started_at : a.position_ms || 0) / 1000))); }
// its keys send the app's own commands; what they did comes back in the status
remoteWrapA('audio', (orig, [what, n]) => { const a = state.audio, pl = audioPl(), app = REMOTE.parts.audio || {}, onoff = k => ['audio', k, a[k] ? 'off' : 'on'];
  if (what === 'track') { const t = pl && pl.tracks[+n]; return t && remoteSend(['audio', 'play', t.name]); }
  if (what === 'play') { const t = pl && pl.tracks[a.track]; return app.playing || app.paused ? remoteSend(['audio', 'pause']) : t ? remoteSend(['audio', 'play', t.name]) : refuse('There is no track to play. Add music at the computer.'); }
  if (what === 'next' || what === 'prev') return remoteSend(['audio', what === 'prev' ? 'previous' : 'next']);
  if (what === 'fwd' || what === 'back') return remoteSend(['audio', 'seek', what === 'fwd' ? '+15' : '-15']);
  if (what === 'through' || what === 'shuffle' || what === 'loop') return remoteSend(onoff(what));
  if (what === 'mute') return remoteSend(['audio', 'mute', a.muted ? 'off' : 'on']);      // the app keeps the level to come back to
  return refuse('Adding music is done at the computer for now.'); });
// a slider is sent once it has settled, not at every step of a drag
// (wrapped when the page starts as a remote: the sliders' own handlers are defined after this file)
function remoteSliders() { if (remoteSliders.done) return; remoteSliders.done = true;
  const later = {}, settle = (k, f) => { clearTimeout(later[k]); later[k] = setTimeout(f, 250); }, seek = TYPED.seek, volume = TYPED.volume, cross = C.crossfade;
  TYPED.seek = v => { if (!REMOTE.on) return seek(v); state.audio.pos = +v; paintAudio(); settle('seek', () => remoteSend(['audio', 'seek', String(Math.round(+v))])); };
  TYPED.volume = (v, el) => { volume(v, el); if (REMOTE.on) settle('volume', () => remoteSend(['audio', 'volume', String(Math.round(+v))])); };
  C.crossfade = v => REMOTE.on ? remoteSend(['audio', 'crossfade', String(+v)]) : cross(v); }

// All Songs: the app's list, each song with its own id (asked for when All Songs is opened, and again when the library changes)
function remoteFetchSongs() {
  const rev = (REMOTE.parts.library || {}).rev;
  remoteSend(['songs'], true).then(r => {
    if (!r.ok || !Array.isArray(r.data)) return;
    REMOTE.library = { rev, songs: r.data.filter(x => x && okId(x.id)).map(x => ({ id: x.id, num: x.number, title: x.title })) };
    remotePlaylists(); fixState(); render();
  });
}
const libKey = id => { const rev = (REMOTE.parts.library || {}).rev; return `sg${id}r${okId(rev) ? rev : 'x'}`; };
function remoteLibrarySongs() {      // every song in a list open here, once each
  const seen = new Map();
  for (const list of [REMOTE.library, ...Object.values(REMOTE.libs)]) for (const x of (list && list.songs) || []) if (!seen.has(x.id)) seen.set(x.id, songObject(libKey(x.id), x.title, { appId: x.id, num: x.num }));
  return [...seen.values()];
}
// A library of its own: its songs, each with its id (a song that can't be found has none, and isn't listed)
function remoteFetchLibrary(name) {
  const rev = (REMOTE.parts.library || {}).rev;
  remoteSend(['library', name], true).then(r => {
    if (!r.ok || !Array.isArray(r.data)) return;
    REMOTE.libs[name] = { rev, songs: r.data.filter(x => x && !x.missing && okId(x.id)).map(x => ({ id: x.id, num: x.number, title: x.title })) };
    remotePlaylists(); fixState(); render();
  });
}

// Slides are read when they come into view, so a long playlist opens at once: two at a time (the server lets four
// commands wait, and Next must never queue behind reads), each tried again a little later if the app was busy.
function remoteLoadVisible() {
  if (!REMOTE.on || REMOTE.status !== 'on') return;
  for (const d of decks()) { const so = d.song; if (so && !so.loaded && !REMOTE.loading.has(so.id) && !REMOTE.queue.some(x => x.id === so.id)) REMOTE.queue.push({ id: so.id, tries: 0 }); }
  remotePump();
}
function remotePump() {
  while (REMOTE.loading.size < 2 && REMOTE.queue.length) {
    const job = REMOTE.queue.shift(), so = byId(DOC.songs, job.id); if (!so || so.loaded) continue;
    REMOTE.loading.add(so.id); REMOTE.maxLoading = Math.max(REMOTE.maxLoading || 0, REMOTE.loading.size);
    const args = so.item ? ['slides', 'item', so.item] : so.appId ? ['slides', so.appId] : ['slides', so.num];
    remoteSend(args, true).then(r => {
      REMOTE.loading.delete(job.id);
      if (r.ok && Array.isArray(r.data)) {
        const slides = r.data.map(x => wordsSlideFrom(x)); REMOTE.slides.set(job.id, { slides });
        const now = byId(DOC.songs, job.id); if (now) { now.slides = slides; now.loaded = true; }
        remoteLive(); render();
      } else if (job.tries < 3 && REMOTE.on) setTimeout(() => { REMOTE.queue.push({ ...job, tries: job.tries + 1 }); remotePump(); }, 800 * (job.tries + 1));
      remotePump();
    });
  }
}

/* ───────── the show's controls, sent to the app ───────── */
function remoteWrap(name, fn) { const orig = window[name]; window[name] = function (...a) { return REMOTE.on ? fn(orig, ...a) : orig(...a); }; }
function remoteWrapA(name, fn) { const orig = A[name]; if (!orig) return; A[name] = function (...a) { return REMOTE.on ? fn(orig, ...a) : orig(...a); }; }
const frontPl = () => DOC.playlists.find(p => p.front) || DOC.playlists[0];
const aimOf = key => { const d = deckByKey(key), p = frontPl(); if (!(d && d.item && state.src.type === 'pl' && playlist() === p)) return null;
  return d.item.uid ? ['--aim', d.item.uid] : d.item.num ? ['--aim', d.item.num, '--rev', REMOTE.parts.playlist ? REMOTE.parts.playlist.rev : ''] : null; };
remoteWrap('go', (orig, key, q, s) => {
  const d = deckByKey(key); if (!d || !d.slides) return;
  if (q == null) q = d.seq.indexOf(s);
  const n = (q >= 0 ? d.seq[q] : s) + 1, aim = aimOf(key);
  state.cue = null;
  if (aim) return remoteSend(['go', aim[0], aim[1], n, ...aim.slice(2)]);
  if (d.song && (d.song.appId || d.song.num)) return remoteSend(['go', d.song.appId || d.song.num, n]);
  refuse('That can’t be put on screen from here yet.');
});
remoteWrap('step', (orig, dir) => {
  if (cueHere() && !state.on) return dir > 0 ? goCue() : moveCue(-1);
  if (Date.now() - REMOTE.lastStep < 90) return;      // a held key repeats faster than the server takes messages
  REMOTE.lastStep = Date.now();
  const aim = aimOf(state.openItem);
  remoteSend([dir > 0 ? 'next' : 'previous', ...(aim || [])]);
});
remoteWrap('clear', (orig, layer) => remoteSend(['clear', layer || 'all']));
remoteWrap('showMedia', (orig, id) => { const m = mediaById(id); if (m) remoteSend(['media', 'show', m.num || m.name]); });
remoteWrap('playMediaPlaylist', (orig, id) => { const mp = byId(DOC.mediaPlaylists, id); if (mp) remoteSend(['media-playlist', 'play', mp.name]); });
remoteWrap('versionUse', (orig, id, keep) => remoteSend(id ? ['version', 'use', versionName(id), ...(keep ? ['--keep'] : [])] : ['version', 'off']));
remoteWrap('mutate', () => refuse(AT_THE_COMPUTER));
remoteWrapA('out', (orig, [k]) => remoteSend([k, state[k] ? 'off' : 'on']));
remoteWrapA('identify', () => remoteSend(['identify', state.identify ? 'off' : 'on']));
remoteWrapA('test', (orig, [k]) => remoteSend(['test', k]));
remoteWrapA('timer', (orig, [what, id]) => { const t = byId(DOC.timers, id); if (t) remoteSend(['timer', what === 'run' ? (t.run ? 'stop' : 'start') : 'reset', t.name]); });
remoteWrapA('message', (orig, [id]) => { const m = byId(DOC.messages, id); if (m) remoteSend(state.message === id ? ['message', 'hide'] : ['message', 'show', m.name]); });
remoteWrapA('prop', (orig, [id]) => { const p = byId(DOC.props, id); if (p) remoteSend(['prop', state.propsOn.includes(id) ? 'hide' : 'show', p.name]); });
remoteWrapA('macro', (orig, [id]) => { const m = byId(DOC.macros, id); if (m) remoteSend(['macro', 'run', m.name]); });
remoteWrapA('video', (orig, [what]) => { const v = state.video;
  remoteSend(what === 'play' ? ['video', v.playing ? 'pause' : 'play'] : what === 'restart' ? ['video', 'restart'] : what === 'fwd' ? ['video', 'seek', v.pos + 10] : what === 'loop' || what === 'mute' ? ['video', what, v[what] ? 'off' : 'on'] : ['video', what]); });
remoteWrapA('stageMsg', () => refuse('Stage messages come to the remote page with the next step. Use the computer for now.'));
// What makes or changes things opens nothing here: it says at once where it is done (a form filled in vain helps nobody).
for (const name of ['transSheet', 'messageEdit', 'timerEdit', 'propNew', 'macroNew', 'newSong', 'importSheet', 'audioNew', 'macroStepDel', 'macroAtStart']) remoteWrapA(name, () => refuse(REASON.edit));
remoteWrapA('addMedia', () => refuse('Adding pictures and videos from here comes with the next step. Add them at the computer for now.'));
remoteWrapA('sampleBackgrounds', () => refuse('The sample backgrounds are added at the computer for now: the media bin’s list, Add the Sample Backgrounds.'));
// A message shown once: the app keeps it as "Quick message"
remoteWrapA('messageOnce', () => ask('Show a message', 'Words', '', v => remoteSend(['message', 'show', '--text', v]), { placeholder: 'The car with the lights on, please', ok: 'Show' }));
remoteWrapA('continuous', () => remoteSend(['continuous', DOC.settings.continuous ? 'off' : 'on']));
remoteWrapA('notesOpen', orig => { const closing = state.notesOpen; orig([]); if (closing) remoteSend(['notices', 'seen'], true); });
remoteWrapA('notesClear', () => remoteSend(['notices', 'clear']));
remoteWrapA('undo', () => refuse('Undo for changes made from here comes with the next step. Use Undo at the computer.'));
remoteWrapA('redo', () => refuse('Undo for changes made from here comes with the next step. Use Undo at the computer.'));
// another playlist: the app runs from the one in front, so choosing one here puts it in front there too
remoteWrapA('src', (orig, [type, id]) => { if (type === 'lib' && id === 'all') { orig([type, id]); if (!REMOTE.library) remoteFetchSongs(); return; }
  if (type === 'lib') { const l = byId(DOC.libraries, id); if (!l) return; orig([type, id]); if (!REMOTE.libs[l.name]) remoteFetchLibrary(l.name); return; } const p = byId(DOC.playlists, id); if (p && !p.front) remoteSend(['playlist', 'open', Number.isSafeInteger(p.num) ? String(p.num) : p.name]); else orig([type, id]); });
remoteWrap('render', orig => {
  if (state.mode !== 'show') { const why = REASON[state.mode] || REASON[state.mode === 'themes' ? 'edit' : 'settings']; state.mode = 'show'; state.sels = []; setTimeout(() => refuse(why), 0); }
  orig(); remoteLoadVisible();
});
// Each mode not yet open to a page says why, where it is chosen.
remoteWrap('renderTop', orig => {
  orig();
  const why = { bible: REASON.bible, edit: REASON.edit };
  for (const b of document.querySelectorAll('#modes [data-a^="mode:"]')) { const m = b.dataset.a.slice(5); if (why[m]) { b.disabled = true; b.title = why[m]; } }
  const gear = $('gear'); if (gear) { gear.disabled = true; gear.title = REASON.settings; }
  const c = $('connBtn'); if (!c) return;
  c.classList.add('rm'); c.classList.toggle('live', REMOTE.status === 'on'); c.classList.toggle('lost', REMOTE.status === 'lost' || REMOTE.status === 'stopped');
  c.querySelector('span').textContent = { on: 'Connected', connecting: 'Connecting…', lost: 'Reconnecting…', stopped: 'Connect', off: 'Not connected' }[REMOTE.status] || 'Not connected';
  c.title = REMOTE.status === 'on' ? `Running SoftPresenter${REMOTE.you ? ` as ${REMOTE.you.device}` : ''}` : REMOTE.status === 'stopped' ? 'Disconnected at the computer: click to connect again' : 'Waiting for SoftPresenter';
});
M.link = () => REMOTE.on ? [
  { head: REMOTE.you ? REMOTE.you.device : 'This device' },
  REMOTE.status === 'stopped' ? { t: 'Connect Again', f: () => { REMOTE.status = 'connecting'; renderTop(); remoteConnect(); } } : null,
  { t: 'Sign Out of This Device…', f: () => confirmDo('Sign out of this device?', 'SoftPresenter forgets this device. Signing in again needs the password.', 'Sign Out', () => remoteSignOut()) },
].filter(Boolean) : [];
function remoteSignOut() {
  if (REMOTE.mode === 'fixture') { REMOTE.status = 'off'; return renderTop(); }
  fetch('/api/logout', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }).finally(() => location.replace('/login'));
}
// The preview shows what the app sends: the audience's picture, as a JPEG (until the first comes, the live slide's own
// picture). The stage's and one screen's pictures come with step 2 of the API.
remoteWrap('monitorHTML', orig => {
  if (state.monitor !== 'Audience' || monScreen()) return html`<div class="bg" style="background:#000"></div><div class="off">This picture comes to the remote page with the next step</div>`;
  if (!REMOTE.preview) return orig();
  return html`<img class="rm-preview" src="${REMOTE.preview}" alt="What the audience sees" draggable="false">${state.audience ? '' : html`<div class="off">The audience screens are off</div>`}`;
});
function remotePaintPreview() { if (REMOTE.on) renderMonitor(); }


// The tabs whose state the page isn't sent yet say so, rather than show anything that isn't the app's.
for (const [tab, what] of [['Stage', 'Choosing the stage layouts'], ['VoxLive', 'VoxLive']]) {
  const orig = TAB[tab];
  TAB[tab] = () => REMOTE.on ? html`<div class="lv-h"><span class="cap">${tab}</span></div><p class="ui-p sub rm-later">${what} comes to the remote page with the next step. Use the computer for now.</p>` : orig();
}

/* ───────── a stand-in for the app's socket, playing its captured samples (the checks, ?remote=fixture) ───────── */
// It replays the app's own clock as captured (each message's `now`), so times are worked out as they would be live.
class FixtureSocket extends EventTarget {
  constructor() { super(); this.readyState = 0; this.binaryType = 'blob'; setTimeout(() => { this.readyState = 1; this.dispatchEvent(new Event('open')); }, 0); }
  emit(data) { setTimeout(() => { if (this.readyState === 1) { const e = new MessageEvent('message', { data: typeof data === 'string' ? data : data }); this.dispatchEvent(e); } }, 0); }
  // ?fixture=es plays the same state in Spanish; ?fixture=first (or first-es) the untouched first playlist ("Service", "Culto")
  get mode() { return new URLSearchParams(location.search).get('fixture') || window.SP_FIXTURE || ''; }
  send(text) {
    const m = JSON.parse(text), F = window.REMOTE_FIXTURE, md = this.mode, es = /es$/.test(md), first = /^first/.test(md) && F.firstStart;
    const AU = F.audio || {};
    if (m.type === 'hello') { window.fixtureEmit = key => this.emit(JSON.stringify((first && first[key]) || AU[key] || F[key]));
      if (md === 'audio' && AU.snapshotAudio) { this.emit(JSON.stringify(AU.snapshotAudio)); if (m.preview) this.picture(); return; }      // a test sends a change or an error
      this.emit(JSON.stringify(first ? (es ? first.snapshotSpanish : first.snapshot) : es && F.snapshotSpanish ? F.snapshotSpanish : F.snapshot)); if (m.preview) this.picture(); return; }
    if (m.type !== 'command') return;
    const a = m.args, reply = r => this.emit(JSON.stringify({ type: 'reply', id: m.id, reply: r }));
    if (a[0] === 'audio' && AU.snapshotAudio) { const then = (r, change) => { reply(r.reply); if (change) this.emit(JSON.stringify(AU[change])); };
      if (a.length === 1) return then(AU.replyAudioList);
      if (a[1] === 'loop' && a[2] === 'on') return then(AU.replyLoopOn, 'changeAfterLoopOn');
      if (a[1] === 'seek') return a[2] === '+15' ? then(AU.replySeekPlus15, 'changeAfterSeek') : +a[2] > 40 ? then(AU.replySeekPastEnd) : reply({ ok: true, text: 'Done.' });
      if (a[1] === 'pause') return then(AU.replyPause, 'changeAfterPause');
      return reply({ ok: true, text: 'Done.' }); }
    if (a[0] === 'menu') return reply(es && F.refusedSpanish ? F.refusedSpanish : F.replies.refused);
    if (a[0] === 'settings' && /^language=/.test(String(a[2] || ''))) return reply(F.refusedLanguageFromPage || { ok: false, text: 'The app’s language is chosen at the computer.' });
    if (first && a[0] === 'playlist' && a[1] === 'open' && a[2] === 'Service') return reply(first.refusedOpenServiceSpanish || { ok: false, text: 'No playlist called “Service”.' });
    if (a[0] === 'slides' && a[1] === 'item') return reply(a[2] === '2' ? F.replies.slidesItem2 : { ok: true, text: '', data: [] });
    if (a[0] === 'slides' && a[1] === ((F.replies.songs.data || []).find(x => x.title === 'Harbour Lights') || {}).id) return reply(F.replies.slidesItem2);      // Harbour Lights, by its id
    if (a[0] === 'playlist' && a.length === 1) return reply(F.replies.playlist);
    if (a[0] === 'songs') return reply(F.replies.songs);
    if (a[0] === 'library') return reply({ ok: true, text: '', data: [...F.replies.songs.data.slice(1).map(x => ({ number: x.number, title: x.title, id: x.id, missing: false })), { number: 9, title: 'A song that has gone', id: null, missing: true }] });      // the shape since 787c892
    if (a[0] === 'groups') return reply({ ok: true, text: '', data: [{ key: 'C', group: 'Chorus', names: ['chorus', 'chorus 1'], color: '#cc4eb4' }, { key: 'V', group: 'Verse', names: ['verse'], color: '#4285f4' }] });      // docs/cli.md's own example
    if (a[0] === 'next') { reply({ ok: true, text: 'On screen: Harbour Lights, slide 2 of 2 (Chorus).' }); this.emit(JSON.stringify(F.change)); return; }
    if (a[0] === 'previous') return reply({ ok: true, text: 'That is the first slide of Harbour Lights, and Continuous is off.', data: { at_end: true } });      // an end reached: ok, the why in text (docs/remote-api.md)
    reply({ ok: true, text: 'Done.' });
  }
  picture() { const c = document.createElement('canvas'); c.width = 64; c.height = 36; const g = c.getContext('2d'); g.fillStyle = '#1d2a3a'; g.fillRect(0, 0, 64, 36);
    c.toBlob(b => this.emit(new Blob([new Uint8Array([1]), b])), 'image/jpeg'); }
  close(code = 1000) { if (this.readyState === 3) return; this.readyState = 3; this.dispatchEvent(new CloseEvent('close', { code })); }
}
// A slide picture the app can't give (a passage only listed, or the app busy) leaves the slide's words showing.
document.addEventListener('error', e => { if (e.target && e.target.matches && e.target.matches('img.sl-img')) e.target.hidden = true; }, true);

/* ═════════ main.js ═════════ */
// Wiring: one handler each for clicks, right-clicks, changes, typing and keys; the clock; and the start.
'use strict';

/* ───────── clicks and menus ───────── */
document.addEventListener('click', e => {
  if (e.target.closest && e.target.closest('.renaming')) return;      // a click inside a name being retyped places the caret, nothing else
  const el = e.target.closest('[data-a]'), inMenu = e.target.closest('#menu');
  if (el) lastAct = { rect: el.getBoundingClientRect(), at: Date.now() };      // where a refusal's cue goes
  if (state.menu && !inMenu && !(el && el.dataset.a.startsWith('menu:'))) closeMenu();
  if (!el) { if (e.target.id === 'scrim') document.body.classList.remove('nav-open'); if (e.target.id === 'sheet') closeSheet(); return; }
  if (el.disabled) return;
  act(el.dataset.a, el, e);
  flushRender();
});
document.addEventListener('pointerdown', e => { pointerHeld = true; pointerEl = e.target; }, true);
const pointerGone = () => { pointerHeld = false; pointerEl = null; setTimeout(flushRender, 0); };
document.addEventListener('pointerup', pointerGone, true); document.addEventListener('pointercancel', pointerGone, true); window.addEventListener('blur', pointerGone);
document.addEventListener('contextmenu', e => {
  const el = e.target.closest('[data-m]'); if (!el) return;
  e.preventDefault();
  const [kind, ...rest] = el.dataset.m.split(':'), args = rest.join(':').split(',');
  if (M[kind]) openMenu(M[kind](args), e.clientX, e.clientY);
});
document.addEventListener('change', e => {
  if (e.target.matches('select, input[type=range]') && !e.target.closest('#sheet')) setTimeout(() => e.target.blur(), 0);      // so the arrow keys go back to running the show
  const el = e.target.closest('[data-c]'); if (!el) return;
  const n = el.dataset.c.indexOf(':'), name = n < 0 ? el.dataset.c : el.dataset.c.slice(0, n), args = n < 0 ? [] : el.dataset.c.slice(n + 1).split(',');
  fromField = pointerHeld && !!pointerEl && pointerEl !== el && !el.contains(pointerEl);      // only when the press that ended the edit landed on something else
  try { if (C[name]) C[name](el.type === 'checkbox' ? el.checked : el.value, args, el); } finally { fromField = false; }
});
A.nav = () => document.body.classList.toggle('nav-open');

/* ───────── typing ───────── */
const TYPED = {
  find: () => renderFind(),
  filter: (v, el) => { state.filter = v; renderShowLeft(); const f = document.querySelector('[data-i="filter"]'); f.focus(); f.setSelectionRange(v.length, v.length); },
  size: v => { DOC.settings.size = +v; document.documentElement.style.setProperty('--tile', v + 'px'); },
  vseek: v => { const r = rowVideo(); if (r) r.v.pos = +v; syncVideo(); paintClock(); },
  volume: v => { const a = state.audio; a.volume = +v; if (+v > 0) a.muted = false; syncAudio(); document.querySelectorAll('[data-i="volume"]').forEach(e => { e.style.setProperty('--p', v + '%'); if (e !== document.activeElement) e.value = v; }); for (const id of ['aVol', 'soundPctM']) if ($(id)) $(id).textContent = v + '%'; },
  soundLevel: v => { DOC.settings.sound.video.level = +v; const e = document.querySelector('[data-i="soundLevel"]'); if (e) e.style.setProperty('--p', v + '%'); if ($('soundPct')) $('soundPct').textContent = v + '%'; syncVideo(); },
  seek: v => { const a = state.audio; a.pos = +v; syncAudio(); paintAudio(); },
  bref: v => typedRef(v),
  biblesFilter: v => { put('biblesList', biblesList(v)); },
  fontFind: v => { put('fontList', fontRows(v)); },
};
document.addEventListener('input', e => { const k = e.target.dataset && e.target.dataset.i; if (k && TYPED[k]) TYPED[k](e.target.value, e.target); });
document.addEventListener('pointerup', e => { if (e.target.matches && e.target.matches('input[type=range]')) e.target.blur(); });

/* ───────── keys ───────── */
const keyAction = k => Object.keys(DOC.settings.keys).find(a => DOC.settings.keys[a].includes(k));
document.addEventListener('keydown', e => {
  const el = e.target, k = e.key, cmd = e.metaKey || e.ctrlKey;
  // The show's keys go by their place on the keyboard, not the letter it types, so they work with any layout (C is the chorus
  // with a Russian layout too, where it types "с"); typing a reference in Bible still takes the letter typed.
  const pk = /^Key[A-Z]$/.test(e.code || '') ? e.code.slice(3).toLowerCase() : e.code === 'Slash' ? '/' : /^Digit[0-9]$/.test(e.code || '') ? e.code.slice(5) : k;
  if (state.capture) { e.preventDefault(); if (k === 'Escape') { closeSheet(); return; } return keyCaptured(k); }
  if (state.menu) {       // a menu is open: the arrows move in it, Enter chooses, Esc closes
    const items = [...document.querySelectorAll('#menu .ui-mi:not(.dim)')], at = items.indexOf(document.activeElement);
    if (k === 'Escape') { e.preventDefault(); return closeMenu(); }
    if (k === 'ArrowDown' || k === 'ArrowUp') { e.preventDefault(); if (items.length) items[(Math.max(at, k === 'ArrowDown' ? -1 : 0) + (k === 'ArrowDown' ? 1 : -1) + items.length) % items.length].focus(); return; }
    if (k === 'ArrowRight' && at >= 0 && items[at].querySelector('.go')) { e.preventDefault(); return items[at].click(); }
    if (k === 'ArrowLeft' && state.menu.stack.length > 1) { e.preventDefault(); return A.menuBack(); }
    if (k === 'Enter' || k === ' ') { e.preventDefault(); if (at >= 0) items[at].click(); return; }
    return;
  }
  if (el.id === 'find') {
    if (k === 'Escape') { el.value = ''; renderFind(); el.blur(); }
    if (k === 'Enter' && cmd) { e.preventDefault(); return findAdd(); }
    if (k === 'Enter') { const r = parseRef(el.value); if (r && !r.missing) { noteTyped(el.value); el.value = ''; renderFind(); el.blur(); return lookUp(r); } const first = document.querySelector('#found [data-a]'); if (first) first.click(); }
    return;
  }
  if (el.id === 'bRef') {
    if (k === 'Enter') { const r = parseRef(el.value); if (r && !r.missing) { clearTimeout(brefTimer); el.blur(); lookUp(r); } else if (B().hint[0] && k === 'Enter') A.bibleHint([0]); }
    if (k === 'Tab' && B().hint.length && !$('bHint').hidden) { e.preventDefault(); A.bibleHint([0]); }
    if (k === 'Escape') el.blur();
    return;
  }
  if (el.isContentEditable) {
    if (k === 'Escape') { el.dataset.cancel = '1'; el.blur(); }      // back as it was, styled words and all
    if (k === 'Enter' && cmd) el.blur();
    if (cmd && !e.altKey && !e.shiftKey && /^[biu]$/i.test(k)) { e.preventDefault(); return A.edToggle([`style.${{ b: 'bold', i: 'italic', u: 'underline' }[k.toLowerCase()]}`]); }      // the words picked, or with none the whole box
    return;
  }
  if (k === 'Escape' && el.matches && el.matches('.sc-name input, .st-in')) { el.value = el.defaultValue; el.blur(); return; }      // a name retyped in place: Escape puts it back
  if (el.matches && el.matches('input:not([type=range]), textarea, select')) { if (k === 'Enter' && state.sheet && el.tagName === 'INPUT') { const b = $('sheet').querySelector('.btn.primary'); if (b) b.click(); } return; }
  if (state.sheet) { if (k === 'Escape') closeSheet(); else if (k === 'Enter' && $('sheet').querySelector('.ui-choice.on:not([disabled])')) { e.preventDefault(); $('sheet').querySelector('.ui-choice.on').click(); } return; }      // Enter takes the choice ringed
  if (k === 'Escape' && state.notesOpen) { e.preventDefault(); return A.notesOpen(); }      // Escape closes the list of notices, and clears nothing
  if (k === 'Escape' && state.mode === 'show' && state.srcMenu) { e.preventDefault(); state.srcMenu = false; return renderShowLeft(); }      // Escape closes the chooser; it never clears the screens then
  if (k === 'Escape' && state.mode === 'edit' && state.reflow) { e.preventDefault(); state.reflow = false; return render(); }      // and the words editor
  if (cmd && !e.altKey && cmdKey(e.code, e.shiftKey)) return e.preventDefault();
  if (e.altKey && !cmd && altKey(e.code)) return e.preventDefault();
  if (k === '?') { e.preventDefault(); return A.keysSheet(); }
  if (state.chord && k === 'Escape') { e.preventDefault(); return endChord(); }
  if (state.chord && /^[1-9]$/.test(k) && !cmd) { e.preventDefault(); return chordPick(+k); }
  if (cmd && k.toLowerCase() === 'z') { e.preventDefault(); return e.shiftKey ? A.redo() : A.undo(); }

  if (state.mode === 'edit' || state.mode === 'themes') {     // here the keyboard belongs to what is being edited, not the show
    const t = edTarget(), picked = edPicked(t);
    if (cmd && k.toLowerCase() === 'c') return A.edObj(['copy']);
    if (cmd && k.toLowerCase() === 'v') return A.edPaste();
    if (cmd && k.toLowerCase() === 'd') { e.preventDefault(); return A.edObj(['dup']); }
    if (cmd && k.toLowerCase() === 'a') { e.preventDefault(); state.sels = t.holder.objects.filter(o => !o.locked).map(o => o.id); return render(); }
    if (cmd) return;
    // Clearing from the Edit view is by keys alone, since its right column is the inspector (the app, e4713a6): the clear keys
    // F1–F6 work as in the Service view; Escape clears everything only when the editor has nothing for it to do (nothing
    // picked, no drag, a fresh press: the first Escape lets go of a picked box, and a held one never clears)
    { const does = keyAction(k), clears = { clearSlide: 'slide', clearMedia: 'media', clearProps: 'props', clearAudio: 'audio', clearMessages: 'messages' };
      if (/^F[1-6]$/.test(k) && does && (clears[does] || does === 'clearAll')) { e.preventDefault(); return clear(clears[does] || 'all'); }
      if (k === 'Escape' && !picked.length && !e.repeat) { e.preventDefault(); return clear('all'); } }
    if (k.toLowerCase() === 't' && !picked.length) return edInsert('text');
    if (!picked.length && (k === 'Delete' || k === 'Backspace')) { e.preventDefault(); return ({ slide: () => A.edSlideOp(['del']), template: () => A.edTplOp(['del']), prop: () => A.edPropOp(['del']), layout: () => A.edLayoutOp(['del']) })[t.type]?.(); }      // with nothing picked on the canvas, Delete acts on the list's picked one, in every list
    if (!picked.length) { if (t.type === 'slide' && (k === 'ArrowUp' || k === 'ArrowDown')) { e.preventDefault(); const n = t.k + (k === 'ArrowUp' ? -1 : 1); if (n >= 0 && n < t.song.slides.length) A.edSlideGo([n]); } return; }
    if (k === 'Escape') { state.sels = []; return render(); }
    if (k === 'Delete' || k === 'Backspace') { e.preventDefault(); return A.edObj(['del']); }
    const by = e.shiftKey ? 1 : 10, mv = { ArrowLeft: [-by, 0], ArrowRight: [by, 0], ArrowUp: [0, -by], ArrowDown: [0, by] }[k];
    if (mv) { e.preventDefault(); const eff = edEff(t); mutate(() => edPicked(edTarget()).filter(o => !o.locked).forEach(o => { const f = eff.find(x => x.id === o.id).frame; o.frame = { ...f, x: f.x + mv[0], y: f.y + mv[1] }; })); }
    return;
  }

  const p = state.picked, d = p.deck && deckByKey(p.deck);
  if (cmd && d && d.song && p.set.length && 'cxv'.includes(k.toLowerCase())) { e.preventDefault(); return slideOp(p.deck, p.set[0], { c: 'copy', x: 'cut', v: 'paste' }[k.toLowerCase()]); }
  if (cmd && k.toLowerCase() === 'a' && state.mode === 'show') { e.preventDefault();      // ⌘A picks every row of the left column's list
    if (state.src.type === 'pl') { state.itemPick = itemIds(); state.itemAnchor = state.itemPick[0]; } else if (state.src.type === 'lib') state.songPick = librarySongs().map(s => s.id); return renderShowLeft(); }
  if (cmd || e.altKey) return;
  if (e.shiftKey && (k === 'ArrowUp' || k === 'ArrowDown') && state.mode === 'show' && state.src.type === 'pl') { e.preventDefault();      // Shift-↑/↓ grow or shrink the pick from the anchor
    const ids = itemIds(), a = state.itemAnchor || state.openItem || ids[0], c = ids[Math.max(0, Math.min(ids.length - 1, ids.indexOf(state.itemCursor || a) + (k === 'ArrowUp' ? -1 : 1)))];
    state.itemAnchor = a; state.itemCursor = c; state.itemPick = itemRange(a, c); return renderShowLeft(); }
  if ((k === 'Delete' || k === 'Backspace') && state.mode === 'show' && state.src.type === 'pl' && !(d && d.song && p.set.length) && !state.mediaPick.length && itemSel().length) { e.preventDefault(); return removeItems(itemSel()); }
  if ((k === 'Delete' || k === 'Backspace') && state.mediaView && state.mediaPick.length && !(d && d.song && p.set.length)) { e.preventDefault(); const mp = byId(DOC.mediaPlaylists, state.mediaView); if (mp.follow) return refuse('It follows a folder: what is in the folder is in it, by name'); const all = [...state.mediaPick]; state.mediaPick = []; return mutate(() => { mp.items = mp.items.filter(x => !all.includes(x)); }); }      // in a media playlist's view, Delete removes from that playlist only, at once
  if ((k === 'Delete' || k === 'Backspace') && !(d && d.song && p.set.length) && (state.songPick.length || state.mediaPick.length)) {      // what is picked in the list last clicked
    e.preventDefault(); const songs = state.songPick.length, ids = songs ? [...state.songPick] : [...state.mediaPick];
    return confirmDo(songs ? `Delete ${ids.length} song${ids.length > 1 ? 's' : ''}?` : `Take ${ids.length} item${ids.length > 1 ? 's' : ''} out of the bin?`, songs ? 'They leave every library; playlists keep their place and show them as missing. This can’t be undone.' : 'The files stay where they are.', 'Delete', () => mutate(() => {
      if (songs) { DOC.songs = DOC.songs.filter(s => !ids.includes(s.id)); DOC.libraries.forEach(l => { l.songs = l.songs.filter(x => !ids.includes(x)); }); state.songPick = []; }
      else { DOC.media = DOC.media.filter(m => !ids.includes(m.id)); DOC.mediaPlaylists.forEach(pl => { pl.items = pl.items.filter(x => !ids.includes(x)); }); state.mediaPick = []; } }));
  }
  if ((k === 'Delete' || k === 'Backspace') && d && d.song && p.set.length) { e.preventDefault(); const key = p.deck, s = p.set[0], n = p.set.length; return undoable(n > 1 ? `Delete ${n} slides?` : 'Delete this slide?', 'Undo brings them back.', 'Delete', () => slideOp(key, s, 'delete')); }
  if (cueHere() && !state.on && (k === 'ArrowRight' || k === 'ArrowLeft')) { e.preventDefault(); return moveCue(k === 'ArrowRight' ? 1 : -1); }      // nothing on screen: the arrows choose, Space or Enter put it on
  const does = keyAction(pk) || keyAction(k), clears = { clearSlide: 'slide', clearMedia: 'media', clearProps: 'props', clearAudio: 'audio', clearMessages: 'messages' };
  if (does === 'find') { e.preventDefault(); return $('find').focus(); }
  if (does === 'next') { e.preventDefault(); return step(1); }
  if (does === 'previous') { e.preventDefault(); return step(-1); }
  if (does === 'clearAll') { e.preventDefault(); state.picked = { deck: null, set: [] }; return clear('all'); }
  if (clears[does]) { e.preventDefault(); return clear(clears[does]); }
  if (state.mode === 'bible' && (k === 'ArrowUp' || k === 'ArrowDown')) { e.preventDefault(); const dir = k === 'ArrowUp' ? -1 : 1;      // as the app has it (a55e297), the owner's rule:
    if (state.on) return step(dir);                                    // something on screen: the arrows stay with it, as Space does
    if (cueHere()) return moveCue(dir);                                // nothing on screen: they move the pick
    const bd = bibleDeck(), at = Math.max(0, bd.slides.findIndex(sl => sl.v === B().ref.from)); setCue('bible', state.bible.opts.per === 'verse' ? at : 0); render(); return followLive('.cued'); }      // nothing picked: pick the passage's first verse
  if (k === 'Enter' && cueHere() && !(el.closest && el.closest('button, a, summary'))) { e.preventDefault(); return goCue(); }      // the cued slide or verse goes on screen
  if (state.mode === 'bible' && k.length === 1 && /[\p{L}\p{N}]/u.test(k)) { e.preventDefault(); const f = $('bRef'); f.focus(); f.value = k; f.setSelectionRange(1, 1); f.dispatchEvent(new Event('input', { bubbles: true })); return; }      // a letter starts a new reference, rather than jumping the song on screen
  if (state.mode !== 'show') return;
  if (k === 'ArrowUp' || k === 'ArrowDown') {       // the item above or below, passing over headers
    e.preventDefault(); const dir = k === 'ArrowUp' ? -1 : 1;
    if (state.src.type === 'lib') { const list = librarySongs(), n = list.findIndex(s => s.id === state.openSong) + dir; if (list[n]) A.openSong([list[n].id], 'key'); return; }      // at either end: nothing, silently
    const all = decks(); for (let n = all.findIndex(x => x.key === state.openItem) + dir; n >= 0 && n < all.length; n += dir) if (all[n].kind !== 'header') return A.openItem([all[n].key, 'key']);
    return;      // at either end of the playlist: nothing, silently
  }
  if (k === 'Enter' && !(el.closest && el.closest('button, a, summary'))) {      // the open item goes on screen from its first slide
    const d = decks().find(x => x.key === (state.src.type === 'lib' ? 'lib-' + state.openSong : state.openItem)); e.preventDefault();
    if (d && d.slides && d.seq.length) return go(d.key, 0); if (d && d.kind === 'media' && d.media) return showMedia(d.media.id); if (d && d.kind === 'mediaPlaylist' && d.mp) return playMediaPlaylist(d.mp.id);
    return;
  }
  const cur = (state.live && deckByKey(state.live.deck)) || decks().find(x => x.key === (state.src.type === 'lib' ? 'lib-' + state.openSong : state.openItem));
  if (!cur || !cur.slides) return;
  if (k === 'Home') return go(cur.key, 0);
  if (k === 'End') return go(cur.key, cur.seq.length - 1);
  if (/^[a-z0-9]$/i.test(pk)) {         // a slide's own hot key first, then the group that letter stands for
    const K = pk.toUpperCase(); let s = cur.slides.findIndex(sl => sl.hotkey === K);
    if (s < 0) { const g = DOC.groups.find(x => x.key === K); if (g) s = cur.slides.findIndex(sl => sl.group === g.name && sl.enabled !== false); }
    if (s >= 0) go(cur.key, null, s);
  }
});

// The audio player's moving parts, without redrawing the tab (a redraw would take the slider out of a dragging hand)
function paintAudio() { const a = state.audio, pl = audioPl(), tr = pl && pl.tracks[a.track], s = $('aSeek'); if (!s) return;
  $('aGone').textContent = mmss(a.pos); $('aLeft').textContent = tr ? '−' + mmss(Math.max(0, tr.length - a.pos)) : '0:00';
  if (document.activeElement !== s || !pointerHeld) s.value = a.pos; s.style.setProperty('--p', (tr && tr.length ? a.pos / tr.length * 100 : 0) + '%'); }

/* ───────── getting around by keys ───────── */
let chordTimer = 0;
function startChord(kind) { endChord(); state.chord = { kind }; document.body.classList.add('chord-' + kind); chordTimer = setTimeout(endChord, 4000); }
function endChord() { clearTimeout(chordTimer); state.chord = null; document.body.classList.remove('chord-tab', 'chord-media'); }
function chordPick(n) {       // after Option-1…7 or Option-M, a number does that thing: start a timer, show a message, run a macro, show a picture
  const kind = state.chord.kind, sel = kind === 'media' ? '#media .clip' : { Timers: '[data-a^="timer:run"]', Messages: '.card [data-a^="message:"]', Props: '.card [data-a^="prop:"]', Macros: '.lv-run', Stage: '.lv-lay', Audio: '[data-a^="audio:track"]', VoxLive: '.card [data-a^="vox:"]' }[state.tab];
  const el = sel && [...document.querySelectorAll(kind === 'media' ? sel : '#tabbody ' + sel)][n - 1];
  if (el) el.click(); startChord(kind);
}
// The keys, as the app has them (keys.rs, master ed55269 and 1949738): Command on a Mac, Ctrl elsewhere. A browser keeps
// ⌘1–⌘3 for its own tabs, so here the views keep ⌥S, ⌥B and ⌥E (the app: ⌘1, ⌘2, ⌘3); everything else is the app's key.
const toBible = () => { state.mode = 'bible'; render(); const f = $('bRef'); if (f) { f.focus(); f.select(); } return true; };
function cmdKey(code, shift) {
  if (shift) switch (code) {
    case 'Digit1': A.out(['audience']); return true;
    case 'Digit2': A.out(['stage']); return true;
    case 'KeyM': if (state.mode === 'show' || state.mode === 'bible') { state.mediaHidden = false; render(); startChord('media'); } return true;
    default: return false;
  }
  switch (code) {
    case 'KeyB': return state.mode === 'edit' || state.mode === 'themes' ? false : toBible();      // in the editor it is Bold
    case 'Comma': A.mode(['settings']); return true;
    case 'KeyF': $('find').focus(); return true;
    case 'KeyP': return altKey('KeyP');
    case 'KeyO': return altKey('KeyO');
    default: return false;
  }
}
function altKey(code) {
  const digit = /^Digit([1-9])$/.exec(code), tabs = [...document.querySelectorAll('#tabs [data-a^="tab:"]')];
  if (digit) { const t = tabs[+digit[1] - 1]; if (!t) return false; t.click(); startChord('tab'); return true; }
  switch (code) {
    case 'KeyS': A.mode(['show']); return true;
    case 'KeyB': return toBible();
    case 'KeyE': A.mode(['edit']); return true;
    case 'KeyP': { state.mode = 'show'; state.srcMenu = true; render(); const b = document.querySelector('.sh-panel .sh-src.on [data-a^="src:"]') || document.querySelector('.sh-panel [data-a^="src:"]'); if (b) b.focus(); return true; }
    case 'KeyO': { if (state.live && state.on && /^(bible|vox)/.test(state.live.deck)) { refuse('The verse or caption on screen has no menu.'); return true; }
      const s = document.querySelector('#slides .live[data-m]') || document.querySelector('#slides .pickd[data-m]'); if (!s) { refuse('Put a slide on screen, or pick one, to open its menu'); return true; }
      const r = s.getBoundingClientRect(), [kind, ...rest] = s.dataset.m.split(':'); openMenu(M[kind](rest.join(':').split(',')), r.left + 24, r.top + 24); return true; }
  }
  return false;
}
const keyList = a => DOC.settings.keys[a].map(keyLabel).join('  ');
const onoffKeys = () => html`<button class="btn ${DOC.settings.showKeys ? 'onkey' : ''}" data-a="showKeys">${sw(DOC.settings.showKeys, 'On', 'Off')}</button>`;
A.showKeys = () => { DOC.settings.showKeys = !DOC.settings.showKeys; render(); if (state.sheet) A.keysSheet(); };
A.keysSheet = () => {
  const groups = [
    ['Running the slides', [[keyList('next'), 'Next slide'], [keyList('previous'), 'Previous slide'], ['↑  ↓', 'The item above or below, cued at its first slide; in Bible, the verse before or after'], ['Enter', 'The cued slide or verse goes on screen (after a search, or ↑ ↓)'], ['← →', 'With nothing on screen, move the cue; Space or Enter puts it on'], ['Home  End', 'First or last slide of the song'], ['A letter', 'Jump to a group (C for the chorus), or a slide’s own hot key']]],
    ['Clearing', [[keyList('clearAll'), 'Clear everything'], [[keyList('clearSlide'), keyList('clearMedia'), keyList('clearProps'), keyList('clearAudio'), keyList('clearMessages')].filter(Boolean).join('  ') || 'The layer keys', 'Clear the slide, media, props, audio, message']]],
    ['Going places', [['⌥S  ⌥B  ⌥E', 'Service, Bible (ready for a reference), Edit (in the app ⌘1 ⌘2 ⌘3; a browser keeps those)'], ['⌘B', 'Bible, too (Bold while editing a slide)'], ['⌘,', 'Settings'], ['⌘P', 'Playlists and libraries'], [keyList('find') + '  ⌘F', 'Find a song, a line or a verse'], ['?', 'This list']]],
    ['The live tools', [['⌥1 … ⌥7', 'Open Timers, Messages, Props, Macros, Stage, Audio, VoxLive'], ['then 1 … 9', 'Start that timer, show that message or prop, run that macro, switch the stage to that layout, play that track'], ['⇧⌘M  then 1 … 9', 'Show that picture or video'], ['⇧⌘1  ⇧⌘2', 'Audience screens, stage screens, on or off'], ['⌘O', 'The menu for the slide on screen']]],
    ['Menus and windows', [['↑  ↓', 'Move'], ['→  ←', 'Open a group of choices; go back'], ['Enter', 'Choose'], ['Esc', 'Close']]],
    ['Changing things', [['⌘Z  ⇧⌘Z', 'Undo, redo'], ['⌘C  ⌘X  ⌘V', 'Copy, cut, paste the picked slides (objects in Edit)'], ['Delete', 'Delete what is picked: what Undo brings back goes at once (playlist items, slides, a media playlist’s items); songs in a library, pictures in the bin and screens ask first'], ['⇧↑  ⇧↓  ⌘A', 'Pick several in the left column: grow the pick, or pick them all'], ['T  ⌘D  ⌘A', 'In Edit: a new text box, duplicate, pick everything'], ['Arrows', 'In Edit: nudge the picked object (Shift for one pixel)'], ['⌘B  ⌘I  ⌘U', 'Typing in a box: bold, italic, underlined, for the words selected (or the whole box)']]],
  ];
  openSheet({ title: 'Keys', wide: true, body: html`<div class="ui-keyson"><b>Show keys on buttons</b><span>A small tag beside each button with its key. Also in Settings › Keys.</span>${onoffKeys()}</div><div class="ui-keys">${groups.map(([name, rows]) => html`<section><span class="cap">${name}</span>${rows.map(([k, what]) => html`<div class="ui-key"><span class="ks">${k.split(/\s{2,}/).map(x => html`<kbd class="st-key mono">${keyText(x)}</kbd>`)}</span><span>${what}</span></div>`)}</section>`)}</div>`,
    note: 'The first nine can be changed in Settings › Keys.', buttons: [{ t: 'Done', primary: true }] });
};

/* ───────── find: songs, lines and verses from one box ───────── */
function renderFind() {
  const box = $('found'), text = $('find').value, want = text.trim().toLowerCase(), ref = parseRef(text);
  box.hidden = !want; if (!want) return;
  // each result says what kind it is at a glance: a note in the library tint for a song, an open book in the scripture tint for the Bible
  const row = (a, title, tag, line) => { const kind = a.startsWith('bible') ? 'bible' : 'song', drag = kind === 'bible' ? 'passage,' + a.slice(10) : 'song,' + a.slice(9).split(',')[0];
    return html`<button class="fd-row ${kind}" data-a="${a}" draggable="true" data-drag="find:${drag}" title="Drag it into a playlist${kind === 'song' ? ' or a library' : ''}"><i class="fd-ico">${kind === 'bible' ? I.book : I.note}</i><b>${title}</b><span class="mono">${tag}</span><em>${line}</em></button>`; };
  const lang = readLang(text), bib = bibleIn(lang) || 'kjv', short = BIBLE_DATA.bibles[bib].short;      // Bible rows in the language being read
  const rows = !ref ? [] : ref.missing ? [html`<div class="none">This sample only has ${BIBLE_SAMPLE}</div>`] : [row(`bibleOpen:${ref.book},${ref.chapter},${ref.from},${ref.to}`, refTitle(ref, bib), `Bible · ${short}`, bibleText(bib, ref.book, ref.chapter, ref.from))];
  let n = 0;
  for (const s of DOC.songs) for (let k = 0; k < s.slides.length && n < 6; k++) if ([s.title, s.slides[k].group, allWords(s.slides[k])].join('\n').toLowerCase().includes(want)) { rows.push(row(`findSong:${s.id},${k}`, s.title, `Song · ${s.slides[k].group}`, slideWords(s.slides[k]).split('\n')[0])); n++; }
  if (/^\d+$/.test(want)) for (const s of DOC.songs) if (s.from && s.from.no === +want) { rows.push(row(`findSong:${s.id},0`, s.title, `Song · № ${s.from.no}`, slideWords(s.slides[0]).split('\n')[0])); n++; }      // a song from the catalog, by the catalog's number for it
  rows.push(...catFind(want, Math.max(2, 6 - n)));      // then the downloaded songbooks' songs, before Bible verses
  if (!ref && want.length >= 3) { let b = 0; const src = BIBLE_DATA.text[bib]; for (const book in src) for (const ch in src[book]) for (const v in src[book][ch]) if (b < 3 && src[book][ch][v].toLowerCase().includes(want)) { rows.push(row(`bibleOpen:${book},${ch},${v},${v}`, refTitle({ book, chapter: +ch, from: +v, to: +v }, bib), `Bible · ${short}`, src[book][ch][v])); b++; } }
  put(box, rows.length ? rows : html`<div class="none">Nothing found</div>`);
}
// ⌘Return in find: the first result goes into the playlist in front (a passage as one item), and nothing goes on screen
function findAdd() {
  const f = $('find'), r = parseRef(f.value), first = document.querySelector('#found [data-a]'), p = state.src.type === 'pl' ? playlist() : DOC.playlists[0];
  let item = null, name = '';
  if (r && !r.missing) { item = { kind: 'passage', passage: { book: r.book, chapter: r.chapter, from: r.from, to: r.to } }; name = refTitle(r); }
  else if (first && first.dataset.a.startsWith('findSong:')) { const id = first.dataset.a.slice(9).split(',')[0]; item = { kind: 'song', song: id }; name = songById(id).title; }
  else if (first && first.dataset.a.startsWith('findCat:')) { const key = first.dataset.a.slice(12).split(',')[0]; item = { kind: 'song', catKey: key }; name = catSong(key).title; }      // from the catalog: it joins the library as it goes in
  else if (first && first.dataset.a.startsWith('bibleOpen:')) { const [book, ch, v] = first.dataset.a.slice(10).split(','); item = { kind: 'passage', passage: { book, chapter: +ch, from: +v, to: +v } }; name = refTitle(item.passage); }
  if (!item) return refuse('Nothing to add');
  mutate(() => { if (item.catKey) { item.song = catJoin(item.catKey).id; delete item.catKey; } p.items.push({ id: newId('it'), ...item }); }); f.value = ''; renderFind(); f.blur(); if (!(state.mode === 'show' && state.src.type === 'pl' && p === playlist())) toast(`${name} added to ${p.name}`);      // only when it went out of sight
}
A.findSong = ([id, k]) => {       // jump to it and cue that slide; nothing goes on screen until Enter
  $('find').value = ''; $('found').hidden = true; $('find').blur(); state.mode = 'show'; deckCache = null;
  let d = decks().find(x => x.song && x.song.id === id);
  if (!d) { state.src = { type: 'lib', id: 'all' }; state.openSong = id; deckCache = null; d = decks()[0]; } else if (state.src.type === 'lib') state.openSong = id; else state.openItem = d.key;
  setCue(d.key, null, +k); render();
  const el = document.querySelector('#slides .cued'); if (el) el.scrollIntoView({ block: 'center' });
};

/* ───────── the clock: it changes only the text it owns, and redraws the rest only when something really changed ───────── */
function paintClock() {
  $('clock').textContent = new Date().toLocaleTimeString('en-GB');
  for (const t of DOC.timers) { const el = document.querySelector(`[data-time="${CSS.escape(t.id)}"]`); if (el) el.textContent = fmt(t.left); }
  const run = DOC.timers.find(t => t.run), rt = $('runTimer'); rt.hidden = !run; if (run) rt.textContent = `${run.name} ${fmt(run.left)}`;
  const r = rowVideo(), m = r && r.m, v = r ? r.v : state.video;
  if (m && $('vSeek')) { if (document.activeElement !== $('vSeek')) $('vSeek').value = v.pos; $('vGone').textContent = mmss(v.pos); $('vLeft').textContent = '-' + mmss(m.length - v.pos); }
  const a = state.audio, pl = audioPl(), tr = pl && pl.tracks[a.track];
  paintAudio();
  if ($('lvAdv')) $('lvAdv').textContent = state.advanceLeft;
  renderMonitor();
}
function tick() {
  if (typeof REMOTE !== 'undefined' && REMOTE.on) return remoteTick();      // the app keeps the time: nothing is worked out here but the clocks
  let changed = false;
  for (const t of DOC.timers) { if (!t.run || LINK.on) continue; /* linked, the app keeps the time */ if (t.kind === 'up') t.left++; else if (t.kind === 'to') t.left = secondsTo(t.at); else if (t.left > 0) t.left--; if (t.kind !== 'up' && t.left === 0) { t.run = false; changed = true; } }
  const m = videoOn(), v = state.video;
  if (m && m.url && v.playing && !$('monVideo').hidden) { const el = $('monVideo'); v.pos = Math.floor(el.currentTime); if (el.ended) { if (state.mediaPl) state.mediaPl.left = 0; else { v.playing = false; changed = true; } } }
  else if (m && v.playing) { v.pos++; if (v.pos >= m.length) { if (state.mediaPl) { v.pos = m.length; state.mediaPl.left = 0; } else if (v.loop) v.pos = 0; else { v.pos = m.length; v.playing = false; changed = true; } } }
  if (state.mediaPl) { const mp = byId(DOC.mediaPlaylists, state.mediaPl.id), cur = mp && mediaById(mp.items[state.mediaPl.n]);
    if (!mp) state.mediaPl = null; else { if (!cur || !cur.video) state.mediaPl.left--; if (state.mediaPl.left <= 0) { state.mediaPl.n = (state.mediaPl.n + 1) % mp.items.length; state.mediaPl.left = 8; showMedia(mp.items[state.mediaPl.n], true); changed = true; } } }
  if (state.ann && state.ann.media) { const am = annMedia(), av = state.annVideo; if (!am) state.ann = null;
    else if (am.video && av && av.playing) { av.pos++; if (av.pos >= am.length) { if (av.loop) av.pos = 0; else { av.pos = am.length; av.playing = false; changed = true; } } } }
  else if (state.ann) { const s = songById(state.ann.song); if (!s) state.ann = null; else if (--state.ann.left <= 0) { state.ann.q = (state.ann.q + 1) % s.slides.length; state.ann.left = s.slides[state.ann.q].advance || 8; changed = true; } }
  if (state.message && state.messageLeft > 0 && --state.messageLeft === 0) { const msg = byId(DOC.messages, state.message); if (msg && msg.once) DOC.messages.splice(DOC.messages.indexOf(msg), 1); state.message = null; changed = true; }
  for (const id of [...state.propsOn]) { const p = byId(DOC.props, id); if (p && p.left > 0 && --p.left === 0) { state.propsOn.splice(state.propsOn.indexOf(id), 1); changed = true; } }
  const a = state.audio, pl = audioPl();
  if (a.playing && pl && pl.tracks[a.track] && pl.tracks[a.track].url) a.pos = Math.floor(audioEl.currentTime);      // a real file keeps its own time
  else if (a.playing && pl && pl.tracks[a.track]) { a.pos++; if (a.pos >= pl.tracks[a.track].length) { audioEnded(a, pl); changed = true; } }
  if (state.advanceLeft > 0 && state.on && --state.advanceLeft === 0) return step(1);
  const typing = document.activeElement && document.activeElement.matches('input, textarea, select, [contenteditable]');
  if (changed && !typing && !state.menu && !drag) render(); else paintClock();
}

/* ───────── start ───────── */
// Served by SoftPresenter, the address's testing switches (?demo=, ?selftest…) are ignored: a demo presses buttons, and there they would press them in the real app.
const q = new URLSearchParams(REMOTE_MODE === 'app' ? '' : location.search);
if (REMOTE_MODE) remoteStart();      // served by SoftPresenter: its own state; only reads are sent until someone presses something
else { go(DOC.playlists[0].items[3].id, null, 2); showMedia('m1'); }          // start as the app's sample does
tick(); setInterval(tick, 1000);
requestAnimationFrame(() => scrollToDeck(state.openItem, false));
const DEMOS = {
  pres: () => presChoice('Sermon.pptx'), preskey: () => presChoice('Easter.key'), presnone: () => { state.noDrawApp = true; presChoice('Easter.key'); }, presin: () => { presImport('Sermon.pptx', 'slides'); },      // Testing aid: ?demo=name opens a state to look at
  bible: () => { lookUp(parseRef('ps 23 1-4')); go('bible', null, 1); }, bibleRows: () => { state.mode = 'bible'; B().fold = false; B().view = 'slides'; render(); }, bibleOptions: () => { state.mode = 'bible'; render(); A.bibleOptions(); },
  bibles: () => { state.mode = 'bible'; render(); A.biblesSheet(); },
  edit: () => { enterEdit(DOC.playlists[0].items[3].id, 2); state.sels = [edTarget().holder.objects[0].id]; state.itab = 'Text'; render(); }, editShape: () => { enterEdit(DOC.playlists[0].items[3].id, 0); edInsert('shape', 'star'); },
  themes: () => { state.mode = 'themes'; render(); }, layout: () => editOther('layout', DOC.stageLayouts[0].id), prop: () => editOther('prop', DOC.props[1].id),
  settings: () => { state.mode = 'settings'; render(); }, integrations: () => { state.mode = 'settings'; state.section = 'Integrations'; render(); }, groups: () => { state.mode = 'settings'; state.section = 'Groups'; render(); },
  coverage: () => { state.mode = 'settings'; state.section = 'Coverage'; render(); }, general: () => { state.mode = 'settings'; state.section = 'General'; render(); },
  video: () => { showMedia('m4'); state.video.pos = 37; render(); }, library: () => { state.srcMenu = true; render(); }, lib: () => A.src(['lib', 'all']),
  menu: () => { const el = document.querySelector('[data-m^="slide:"]'), r = el.getBoundingClientRect(); openMenu(M.slide(el.dataset.m.slice(6).split(',')), r.left + 60, r.top + 40); },
  easy: () => { DOC.settings.view = 'easy'; render(); }, table: () => { DOC.settings.view = 'table'; render(); }, newsong: () => A.newSong(), import: () => A.importSheet(), setup: () => A.setup([0]),
  audio: () => { state.tab = 'Audio'; A.audio(['track', '1']); }, macros: () => { state.tab = 'Macros'; render(); }, props: () => { state.tab = 'Props'; A.prop([DOC.props[0].id]); A.prop([DOC.props[1].id]); A.message([DOC.messages[1].id]); },
  stage: () => { state.monitor = 'Stage'; state.tab = 'Stage'; DOC.timers[0].run = true; render(); }, vox: () => { state.tab = 'VoxLive'; A.vox(['v1']); }, voxlook: () => { state.mode = 'settings'; state.section = 'Integrations'; render(); A.voxLook(['v1']); },
  keys: () => { state.mode = 'settings'; state.section = 'Keys'; render(); }, font: () => { DEMOS.edit(); A.fontSheet(); },
  build: () => { const sl = DOC.songs[1].slides[2]; sl.objects[1].build = { in: 'Fly In', from: 'Bottom', when: 'On a click of Next', out: 'None' }; go(DOC.playlists[0].items[3].id, null, 2); step(1); render(); },
  john1: () => { lookUp(parseRef('john 1')); B().view = 'slides'; const t0 = performance.now(); render(); document.title = 'RENDER ' + Math.round(performance.now() - t0) + 'ms'; },
  dbg: () => { const sc = document.createElement('script'); sc.src = 'dbg.js'; document.body.appendChild(sc); },
  identify: () => { state.mode = 'settings'; state.identify = true; render(); }, ann: () => { state.ann = { song: DOC.songs[5].id, q: 0, left: 8 }; render(); },
};
if (DEMOS[q.get('demo')]) DEMOS[q.get('demo')]();
// The two screen switches are written in the page itself: their keys, as the computer names them
for (const [id, k] of [['outAudience', '⇧⌘1'], ['outStage', '⇧⌘2']]) { const b = $(id); if (b) { b.title = b.title.replace(k, keyText(k)); const t = b.querySelector('.kh'); if (t) t.textContent = keyText(k); } }
if (q.has('lang') || q.has('probe')) { const sc = document.createElement('script'); sc.src = 'langtool.js'; document.body.appendChild(sc); }      // Testing aid: the app's words in another language, or stretched, and where they don't fit
if (q.has('linktest')) { const sc = document.createElement('script'); sc.src = 'linktest.js'; document.body.appendChild(sc); }      // Testing aid
if (q.has('selftest')) { const sc = document.createElement('script'); sc.src = 'selftest.js'; document.body.appendChild(sc); }
if (q.has('remotetest')) { const sc = document.createElement('script'); sc.src = 'remotetest.js'; document.body.appendChild(sc); }      // Testing aid: with ?remote=fixture
// A double-click on an object's name renames it.
document.addEventListener('dblclick', e => { { const strip = e.target.closest && e.target.closest('.edslide .strip'); if (strip) return A.edGroupRename([strip.closest('.edslide').dataset.drag.split(':')[1]]); } const el = e.target.closest('[data-a^="edPick:"]'); if (el) { state.sels = [el.dataset.a.slice(7)]; A.edObj(['rename']); } });
DEMOS.keys2 = () => { DOC.settings.showKeys = true; altKey('Digit4'); };      // Testing aid: the key hints and the numbered picks
DEMOS.keysheet = () => A.keysSheet();
if (DEMOS[q.get('demo')] && (q.get('demo') === 'keys2' || q.get('demo') === 'keysheet')) DEMOS[q.get('demo')]();
DEMOS.arr = () => { const g = DOC.songs[0].arrangements[0].groups; g.splice(1, 1, 'Chorus', 'Chorus', 'Chorus'); deckCache = null; go(decks().find(d => d.song === DOC.songs[0]).key, 3); };      // Testing aid: a group sung three times
if (q.get('demo') === 'arr') DEMOS.arr();

DEMOS.sound = () => { state.mode = 'settings'; state.section = 'Sound'; DOC.settings.sound.music = { to: 'usb', pair: 1 }; DOC.settings.sound.video.to = 'tv'; render(); };
if (q.get('demo') === 'sound') DEMOS.sound();
// Testing aids for the design review: states that had no demo of their own
Object.assign(DEMOS, { messages: () => { state.tab = 'Messages'; render(); }, timers: () => { state.tab = 'Timers'; DOC.timers[0].run = true; render(); }, webctl: () => { state.mode = 'settings'; state.section = 'Web control'; render(); },
  bslides: () => { state.mode = 'bible'; B().view = 'slides'; render(); }, chooser: () => { state.srcMenu = true; render(); }, onescreen: () => { state.monScreen = DOC.screens[1].id; render(); }, cleared: () => { A.clear(['all']); },
  editwords: () => { A.mode(['edit']); const b = document.querySelector('[data-a^="edWords"], [data-a="words"]'); if (b) b.click(); }, scropen: () => { state.mode = 'settings'; state.section = 'Screens'; state.scrOpen = DOC.screens[0].id; render(); } });
if (['messages', 'timers', 'webctl', 'bslides', 'chooser', 'onescreen', 'cleared', 'editwords', 'scropen'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
if (q.has('dump')) setTimeout(() => { const sc = document.createElement('script'); sc.src = 'handoff/dump.js'; document.body.appendChild(sc); }, 400);      // the hand-off's measuring pass
if (q.has('schema')) setTimeout(() => { const sc = document.createElement('script'); sc.src = 'handoff/schema.js'; document.body.appendChild(sc); }, 300);
// Testing aids: the find list with both kinds, a cued verse, a cued slide
Object.assign(DEMOS, { findmix: () => { $('find').value = 'light'; renderFind(); }, cuebible: () => { lookUp(parseRef('gen 1 4')); }, cueshow: () => { A.openItem([decks().find(d => d.song).key, 'key']); } });
if (['findmix', 'cuebible', 'cueshow'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
Object.assign(DEMOS, { blists: () => { lookUp(parseRef('jn 3 16')); DOC.settings.bibleBooks = 'lists'; render(); }, bru: () => { lookUp(parseRef('jn 3 16')); DOC.settings.bibleLang = 'ru'; render(); } });      // Testing aids: the book lists, and the page in Russian
if (['blists', 'bru'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
// Testing aids: the Screens page in each state the hand-off measures
Object.assign(DEMOS, {
  scrndi: () => { state.mode = 'settings'; state.section = 'Screens'; state.scrOpen = DOC.screens[1].id; render(); },
  scrstage: () => { state.mode = 'settings'; state.section = 'Screens'; state.scrOpen = DOC.screens.find(x => x.kind === 'Stage').id; render(); },
  scrdeck: () => { DOC.screens.splice(2, 0, { id: 'scr-sw', kind: 'Audience', name: 'Switcher', to: 'Lyrics', format: '1080i59.94', formatFrom: 'card', keyer: 'fill', on: true, theme: DOC.themes[1] ? DOC.themes[1].id : null, layers: ['slide', 'props', 'messages'], colour: '#000000', edges: [0, 0, 0, 0], bgMode: 'solid', tint: '#4fc3f7' }); state.mode = 'settings'; state.section = 'Screens'; state.scrOpen = 'scr-sw'; render(); },
  scrkey: () => { DOC.screens.splice(2, 0, { id: 'scr-k', kind: 'Audience', name: 'Stream key', to: 'NDI', ndiName: 'SoftPresenter Stream key', on: true, theme: null, layers: [...ALL_LAYERS], colour: '#000000', edges: [0, 0, 0, 0], bgMode: 'solid', keyOf: DOC.screens[1].id, tint: '#ffd54f' }); state.mode = 'settings'; state.section = 'Screens'; state.scrOpen = 'scr-k'; render(); },
  scrclash: () => { DOC.screens[2].to = 'Display: HP 27f'; DOC.screens[2].disconnected = false; state.mode = 'settings'; state.section = 'Screens'; render(); },
  scrident: () => { state.identify = true; state.mode = 'settings'; state.section = 'Screens'; render(); },
  scrnostage: () => { DOC.screens = DOC.screens.filter(x => x.kind !== 'Stage'); state.mode = 'settings'; state.section = 'Screens'; render(); },
});
if (['scrndi', 'scrstage', 'scrdeck', 'scrkey', 'scrclash', 'scrident', 'scrnostage'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
Object.assign(DEMOS, { picks: () => { const it = playlist().items; state.openItem = it[2].id; state.itemAnchor = it[2].id; state.itemPick = [it[2].id, it[3].id, it[5].id]; render(); } });      // Testing aid: several items picked
if (q.get('demo') === 'picks') DEMOS.picks();
Object.assign(DEMOS, { renaming: () => { const s = DOC.songs[1]; setTimeout(() => { const it = M.song([s.id]).find(x => x && x.t === 'Rename'); it.f(); }, 50); } });      // Testing aid: a song being renamed in place
if (q.get('demo') === 'renaming') DEMOS.renaming();
Object.assign(DEMOS, { scrtech: () => { DOC.settings.scrDetails = true; DOC.screens[0].bgMode = 'clear'; state.mode = 'settings'; state.section = 'Screens'; render(); } });      // Testing aid: Details on, and a screen changed in Fine-tune
if (q.get('demo') === 'scrtech') DEMOS.scrtech();
Object.assign(DEMOS, { macrostart: () => { state.tab = 'Macros'; state.macroOpen = DOC.macros[0].id; DOC.macros[0].atStart = true; render(); } });      // Testing aid: a macro that runs when SoftPresenter opens
if (q.get('demo') === 'macrostart') DEMOS.macrostart();
Object.assign(DEMOS, {      // Testing aids: theme versions
  vers1: () => { playlist().items[3].version = 'ver-offering'; render(); },
  vers2: () => { playlist().items[3].version = 'ver-offering'; render(); go(playlist().items[3].id, 0); },
  vers3: () => { A.mode(['edit']); A.edKind(['themes']); state.themeSel = { theme: DOC.themes[0].id, tpl: DOC.themes[0].templates[3].id }; render(); },
  vers4: () => { A.mode(['edit']); A.edKind(['themes']); state.themeSel = { theme: DOC.themes[1].id, tpl: DOC.themes[1].templates[2].id }; render(); } });
if (['vers1', 'vers2', 'vers3', 'vers4'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
Object.assign(DEMOS, { vers5: () => { A.mode(['edit']); A.edKind(['themes']); state.themeSel = { theme: DOC.themes[1].id, tpl: null }; render(); DOC.themes[1].templates = DOC.themes[1].templates.filter(t => !t.version); render(); A.versionNew(); } });      // Testing aid: adding a version to a theme
if (q.get('demo') === 'vers5') DEMOS.vers5();
Object.assign(DEMOS, { itemmenu: () => { playlist().items[3].version = 'ver-offering'; render(); setTimeout(() => { const b = document.querySelector(`[data-a="menu:item,${playlist().items[3].id}"]`); if (b) b.click(); }, 60); } });      // Testing aid: an item's menu with its Version section
if (q.get('demo') === 'itemmenu') DEMOS.itemmenu();
Object.assign(DEMOS, {      // Testing aids: notices, the status button and its list
  notes: () => { toast('“Harbour Lights” added to Wednesday Evening'); toast('Reading Ukrainian_Ogienko.xml…'); toast('Installed Ukrainian Ogienko'); trouble('SoftPresenter didn’t answer. Open it, then connect again, and it will carry on from where it stopped.'); NOTES.forEach((n, k) => { n.time = new Date(Date.now() - (k + 1) * 4 * 60000); }); NOTES[3].seen = true; render(); setTimeout(() => { state.notesOpen = true; renderNote(); }, 60); },
  notesnew: () => { toast('“Harbour Lights” added to Wednesday Evening'); trouble('SoftPresenter didn’t answer. Open it, then connect again.'); render(); },
  // A newer build installed over the running copy (as built, 24 November): said once, as information; nothing restarts by itself
  notenewer: () => { toast('A newer SoftPresenter has been installed. Quit and reopen SoftPresenter to use it.'); render(); },
  // The sp command installed by itself at the first start from an installed place (as built, 24 November, 7557fd8): one notice
  // Phone Screens (as built, 24 November): the pane switched on and waiting; a device asking; a box with a device showing
  mirror: () => { mirrorSet().on = true; state.mode = 'settings'; state.section = 'Phone Screens'; render(); },
  mirrorask: () => { mirrorSet().on = true; state.mirrorAsk = 'Ana’s iPhone'; render(); },
  mirrorbox: () => { mirrorSet().on = true; state.mirrorDevice = 'Ana’s iPhone'; DEMOS.edit(); edInsert('mirror', null); render(); },
  spready: () => { toast(WINDOWS ? 'The sp command is ready for the terminal (%LOCALAPPDATA%\\SoftPresenter\\bin\\sp.cmd), and its folder is on your PATH. In PowerShell, type softpresenter instead. Settings › Integrations › Command Line can remove it.'
    : 'The sp command is ready for the terminal (~/.local/bin/sp). SoftPresenter added a line for it to ~/.zshrc. Settings › Integrations › Command Line can remove it.'); render(); },
  noteask: () => { state.changedOutside = DOC.songs[1].id; render(); },
  noterefuse: () => { render(); setTimeout(() => { go(playlist().items[7].id, 2); refuse('That playlist is empty'); }, 80); } });
if (['notes', 'notesnew', 'noteask', 'noterefuse', 'notenewer', 'spready', 'mirror', 'mirrorask', 'mirrorbox'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
Object.assign(DEMOS, {      // Testing aid: words styled inside a box, some of them selected (mixed looks) for the inspector
  words: () => { enterEdit(null, 0, DOC.songs[1].id); const m = DOC.songs[1].slides[0].objects.find(o => o.role === 'main'), t = m.text, a = t.indexOf('the light'), b = t.indexOf('You are');
    m.spans = [{ from: a, to: a + 9, italic: true, size: 104, color: '#8fc8ff' }]; state.sels = [m.id]; state.itab = 'Text'; state.tsel = { id: m.id, from: b, to: a + 9 }; render(); } });
if (q.get('demo') === 'words') DEMOS.words();
// Testing aids: the first-run setup and the song catalog (designed 23 November; awaiting the owner)
const suDemo = (step, langs, more) => { catState().have = {}; A.setup([0]); const u = state.su; u.langs = langs; u.songs = Object.fromEntries(CAT_LANGS.map(l => [l, langs.includes(l)])); suLangsDone(); u.step = step; if (more) more(u); suOpen(); };
const catDemoSong = () => { let s; mutate(() => { s = catJoin('ru1'); s.from.changed = true; s.from.newer = true; }); return s; };
Object.assign(DEMOS, {
  su0: () => suDemo(0, ['ru', 'en']), su1: () => suDemo(1, ['ru', 'en']), su1off: () => suDemo(1, ['ru', 'en'], () => { catState().offline = true; }),
  su1none: () => { CATALOG.langs.uk.songs = 0; suDemo(1, ['uk', 'en']); },      // a language with no songs yet, Ukrainian standing in
  su2: () => suDemo(2, ['ru', 'en'], () => { catState().dl = { ru: { pct: 38 }, en: { pct: 71 } }; }), su2uk: () => suDemo(2, ['uk', 'en']),
  su3: () => suDemo(3, ['ru', 'en'], () => { catState().dl = { ru: { pct: 54 } }; catState().have.en = { at: Date.now() }; }),
  su3one: () => suDemo(3, ['ru', 'en'], u => { u.one = 'Built-in Display'; u.aud = 'In a window for now'; u.stage = null; }),      // only the display SoftPresenter's window is on
  su4: () => suDemo(4, ['ru', 'en'], () => { catState().dl = { ru: { pct: 62 } }; catState().have.en = { at: Date.now() }; }),
  su4skip: () => suDemo(4, ['en'], u => { u.skipped = { 1: true, 2: true, 3: true }; }),
  su4fail: () => suDemo(4, ['ru', 'en'], () => { catState().fail = { ru: 'the connection dropped part way' }; catState().have.en = { at: Date.now() }; }),
  suagain: () => { catSample(); DOC.settings.setupDone = true; DOC.settings.langs = ['ru', 'en']; A.setup([1]); },
  catfind: () => { catSample(); $('find').value = '245'; renderFind(); }, catfind2: () => { catSample(); $('find').value = 'harbour'; renderFind(); },
  catlist: () => { catSample(); A.src(['cat', 'wla']); A.openCat(['wla-ru1']); },
  catlib: () => { catSample(); DOC.settings.langs = ['ru', 'en']; const s = catDemoSong(); A.src(['lib', DOC.libraries.find(l => l.catalog).id]); A.openSong([s.id]); },
  catinfo: () => { catSample(); DOC.settings.langs = ['ru', 'en']; const s = catDemoSong(); A.src(['lib', DOC.libraries.find(l => l.catalog).id]); A.openSong([s.id]); A.songInfo([s.id]); },
  catchooser: () => { catSample(); state.srcMenu = true; render(); },
  catset: () => { catSample(); state.mode = 'settings'; state.section = 'Integrations'; state.integ = 'wla'; render(); },
});
if (/^(su|cat)/.test(q.get('demo') || '') && DEMOS[q.get('demo')]) DEMOS[q.get('demo')]();
// Testing aid: the sample backgrounds: offered in the media bin's list, offered by an empty bin, and once added
Object.assign(DEMOS, {
  samplesmenu: () => setTimeout(() => { const el = document.querySelector('[data-a="menu:mediaSrc"]'), r = el.getBoundingClientRect(); openMenu(M.mediaSrc(), r.left, r.bottom + 4); }, 50),
  samplesempty: () => { DOC.media = []; DOC.mediaPlaylists.forEach(p => { p.items = []; }); state.mediaOn = null; render(); },
  samples: () => A.sampleBackgrounds(),
});
if (/^samples/.test(q.get('demo') || '') && DEMOS[q.get('demo')]) DEMOS[q.get('demo')]();
// Testing aid: an NDI source in a slide: just added (its source to choose), finding none, without NDI, chosen, and stopped
Object.assign(DEMOS, {
  ndiadd: () => { DEMOS.edit(); edInsert('ndi', null); render(); },
  ndinone: () => { state.ndi = 'looking'; DEMOS.edit(); edInsert('ndi', null); render(); },
  ndimissing: () => { state.ndi = 'none'; DEMOS.edit(); edInsert('ndi', null); render(); },
  ndiobj: () => { DEMOS.edit(); edInsert('ndi', 'STREAM-PC (Camera 1)'); render(); },
  ndilost: () => { DEMOS.edit(); edInsert('ndi', 'STREAM-PC (Camera 1)'); state.ndiLost = ['STREAM-PC (Camera 1)']; trouble('NDI “Camera 1” stopped sending. Its slides show without it until it is back.'); render(); },
  webadd: () => { DEMOS.edit(); edInsert('web', ''); render(); },
});
if (/^(ndi|webadd)/.test(q.get('demo') || '') && DEMOS[q.get('demo')]) DEMOS[q.get('demo')]();
// Testing aid: the inspector's tabs for each kind: a picture, a web page's Box, and two kinds picked together
Object.assign(DEMOS, {
  insppic: () => { DEMOS.edit(); edInsert('media', 'm1'); render(); },
  inspbox: () => { DEMOS.edit(); edInsert('web', 'https://example.org/live'); state.itab = 'Box'; render(); },
  inspmixed: () => { DEMOS.edit(); edInsert('web', 'https://example.org/live'); state.sels.push(edTarget().holder.objects[0].id); render(); },
});
if (/^insp/.test(q.get('demo') || '') && DEMOS[q.get('demo')]) DEMOS[q.get('demo')]();
// Testing aid: the homes of what the Mac menu gave up: Settings › About, and the media strip folded
Object.assign(DEMOS, { about: () => { state.mode = 'settings'; state.section = 'About'; render(); }, mediafold: () => { state.mediaHidden = true; render(); } });
if (['about', 'mediafold'].includes(q.get('demo'))) DEMOS[q.get('demo')]();
