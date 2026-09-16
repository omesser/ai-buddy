// Local clock stamps for Chat surface turns. Its own module because chat.js
// reaches window.__TAURI__ as it loads and cannot be imported outside a
// webview; this can, so it has a test.

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

function pad2(n) {
  return String(n).padStart(2, "0");
}

function localHm(at) {
  return `${pad2(at.getHours())}:${pad2(at.getMinutes())}`;
}

function dayMonth(at) {
  return `${at.getDate()} ${MONTHS[at.getMonth()]}`;
}

function sameLocalDay(a, b) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

// Drawn on a 420-point Chat surface, so the label is five characters until the
// local day changes; then the date prefixes once and later lines go back to
// HH:mm. Year and seconds live on title. A first line is HH:mm too.
export function stampWhen(at, previousAt) {
  const time = localHm(at);
  const dayChanged = previousAt !== null && !sameLocalDay(previousAt, at);
  return {
    label: dayChanged ? `${dayMonth(at)} ${time}` : time,
    datetime: at.toISOString(),
    title: `${dayMonth(at)} ${at.getFullYear()}, ${time}:${pad2(at.getSeconds())}`,
  };
}
