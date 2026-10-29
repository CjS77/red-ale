// Search for the red-ale Manual: plain term matching over search-index.js. No dependencies, no requests.
(function () {
  "use strict";
  var pages = window.redAleIndex || [];
  var form = document.getElementById("search-form");
  var input = document.getElementById("q");
  var status = document.getElementById("search-status");
  var list = document.getElementById("search-results");
  var LIMIT = 50;

  // The same light stemming the Manual's own search uses: plurals to singular.
  function stem(word) {
    if (word.length > 4 && /ies$/.test(word)) { return word.slice(0, -3) + "y"; }
    if (word.length > 3 && /s$/.test(word) && !/(ss|us|is)$/.test(word)) { return word.slice(0, -1); }
    return word;
  }

  function queryWords(text) {
    var words = text.toLowerCase().split(/[^a-z0-9.]+/).map(function (w) {
      return w.replace(/^\.+|\.+$/g, "");
    });
    return words.filter(function (w) { return w.length > 1; }).map(stem);
  }

  function path(page) { return page.id === "start" ? "index.html" : page.id + ".html"; }

  // 0 when the page lacks the word; otherwise the higher the better: title, then Page ID, summary, then body terms.
  function scoreWord(page, word, terms) {
    var score = 0;
    if (page.title.toLowerCase().indexOf(word) >= 0) { score += 6; }
    if (page.id.indexOf(word) >= 0) { score += 4; }
    if (page.summary.toLowerCase().indexOf(word) >= 0) { score += 2; }
    if (terms.indexOf(" " + word + " ") >= 0) { score += 2; }
    else if (terms.indexOf(" " + word) >= 0) { score += 1; }
    return score;
  }

  function score(page, words) {
    var terms = " " + page.terms + " ";
    var total = 0;
    for (var i = 0; i < words.length; i++) {
      var s = scoreWord(page, words[i], terms);
      if (s === 0) { return 0; }
      total += s;
    }
    return total;
  }

  function search(text) {
    var words = queryWords(text);
    if (words.length === 0) { return []; }
    return pages
      .map(function (page) { return { page: page, score: score(page, words) }; })
      .filter(function (hit) { return hit.score > 0; })
      .sort(function (a, b) { return b.score - a.score || (a.page.id < b.page.id ? -1 : 1); })
      .slice(0, LIMIT)
      .map(function (hit) { return hit.page; });
  }

  function show(text) {
    var hits = search(text);
    list.textContent = "";
    hits.forEach(function (page) {
      var item = document.createElement("li");
      var link = document.createElement("a");
      var about = document.createElement("span");
      link.href = path(page);
      link.textContent = page.title;
      about.className = "about";
      about.textContent = page.kind + ": " + page.summary;
      item.appendChild(link);
      item.appendChild(about);
      list.appendChild(item);
    });
    status.textContent = text.trim() === "" ? "" : hits.length + (hits.length === 1 ? " page matches." : " pages match.");
  }

  var asked = /[?&]q=([^&]*)/.exec(window.location.search);
  if (asked) { input.value = decodeURIComponent(asked[1].replace(/\+/g, " ")); }
  input.addEventListener("input", function () { show(input.value); });
  form.addEventListener("submit", function (event) { event.preventDefault(); show(input.value); });
  show(input.value);
})();
