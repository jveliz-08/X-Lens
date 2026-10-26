'use strict';
const $ = id => document.getElementById(id);
let token = '', timer, urls = [], generation = 0;
function revoke() { urls.forEach(URL.revokeObjectURL); urls = []; }
function lock() { generation++; clearTimeout(timer); token = ''; $('token').value = ''; revoke(); $('cameras').replaceChildren(); $('login').hidden = false; $('dashboard').hidden = true; $('connection').textContent = 'LOCKED'; }
async function request(path) { const r = await fetch(path, {headers:{Authorization:`Bearer ${token}`}, cache:'no-store', signal:AbortSignal.timeout(6000)}); if (!r.ok) throw Error(r.status === 401 ? 'Access denied. Check your viewer token.' : `Request failed (${r.status}).`); return r; }
async function refresh() {
 const g = generation;
 const pending = [];
 try {
  const devices = await (await request('/api/devices')).json();
  const signals = await (await request('/api/security')).json();
  const cards = [];
  for (const d of devices) {
   const card = document.createElement('article'); card.className = 'camera';
   if (d.sequence) { const r = await request(`/api/latest/${d.id}`); const url = URL.createObjectURL(await r.blob()); pending.push(url); const img = document.createElement('img'); img.src = url; img.alt = `Latest snapshot from ${d.id}`; card.append(img); }
   else { const empty = document.createElement('div'); empty.className = 'empty'; empty.textContent = 'Waiting for first snapshot'; card.append(empty); }
   const name = document.createElement('h3'); name.textContent = d.id;
   const info = document.createElement('p'); info.textContent = `Sequence ${d.sequence} · ${d.retained} retained`;
   card.append(name,info); cards.push(card);
  }
  if (g !== generation) { pending.forEach(URL.revokeObjectURL); return; }
  $('security-signals').textContent = `Local security signals · ${signals.auth_failures} authentication failures · ${signals.devices.reduce((n,d)=>n+d.replays,0)} replay rejections · Since server start; review signals before taking action.`;
  revoke(); urls = pending; $('cameras').replaceChildren(...cards); $('count').textContent = `(${devices.length})`; $('status').textContent = `Last checked ${new Date().toLocaleTimeString()} · Sequence indicates capture progress, not camera health.`;
  $('login').hidden = true; $('dashboard').hidden = false; $('connection').textContent = 'CONNECTED';
 } catch (e) { pending.forEach(URL.revokeObjectURL); if (g !== generation) return; $('status').textContent = e.message; $('connection').textContent = 'UNAVAILABLE'; }
 if (g === generation && token) timer = setTimeout(refresh,5000);
}
$('connect').onclick = () => { clearTimeout(timer); generation++; token = $('token').value.trim(); $('token').value = ''; if (!/^[a-fA-F0-9]{64}$/.test(token)) { $('status').textContent = 'Enter a 64-character hexadecimal viewer token.'; token = ''; return; } refresh(); };
$('token').onkeydown = e => { if (e.key === 'Enter') $('connect').click(); };
$('lock').onclick = lock;
window.addEventListener('pagehide',lock);
