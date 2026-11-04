'use strict';
const $ = id => document.getElementById(id);
let token = '', pollTimer, generation = 0, devices = [], savedRules = [], draft = null;
const feeds = new Map(); let clockTimer;
let feedback = new Map(), seenNotices = null;
let operator = '', activity = null, timelineKey = '', thumbs = new Map(), seenAlerts = null, player = null, askEpoch = 0;
let refreshEpoch = 0, proposalEpoch = 0, proposalController;
let tab = 'overview', stream = null, cameraEpoch = 0, captureTimer, uploadController, uploading = false;
let speech = null, speechEpoch = 0, speechTimer;
const inflight = new Set();
const text = (tag, value, className) => { const e = document.createElement(tag); e.textContent = value; if(className) e.className=className; return e; };
const message = (id, value) => { $(id).textContent=value; };
// Live feeds: one authenticated multipart/x-mixed-replace stream per active device, read with fetch
// so the viewer token stays in a header (an <img src> stream would need it in the URL).
function feedTile(id) {
 const f={id,url:'',seq:'',capturedAt:0,arrivals:[],controller:null,retry:null};
 f.tile=text('article','','camera');f.tile.dataset.state='connecting';
 const stage=text('div','','stage');f.img=document.createElement('img');f.img.alt=`Live view from ${id}`;f.img.hidden=true;
 f.waiting=text('div','Connecting…','stage-empty');
 const top=text('div','','overlay');f.badge=text('span','CONNECTING','live-badge');f.rec=text('span','REC','rec');f.rec.hidden=true;f.stats=text('span','','cam-stats');top.append(f.badge,text('span',id,'cam-name'),f.rec,f.stats);
 const meter=text('div','','meter');f.level=text('span','');meter.append(f.level);meter.title='Motion detected on the camera';
 f.zones=text('div','','zone-overlay');f.zones.append(...Array.from({length:48},()=>text('span','')));
 stage.append(f.img,f.zones,f.waiting,top,meter);
 const bar=text('div','','camera-bar');f.meta=text('p','Waiting for the first frame','muted');const actions=text('div','','button-row');
 const full=text('button','Fullscreen');full.onclick=()=>{if(document.fullscreenElement)document.exitFullscreen();else stage.requestFullscreen?.().catch(()=>{});};
 f.save=text('button','Save frame');f.save.disabled=true;f.save.onclick=()=>{if(!f.url)return;const a=document.createElement('a');a.href=f.url;a.download=`${id}-${f.seq}.jpg`;a.click();};
 const zones=text('button','Zones');zones.title='Ignore parts of the scene when detecting motion';zones.onclick=()=>openZones(id);
 actions.append(zones,full,f.save);bar.append(f.meta,actions);f.tile.append(stage,bar);paintZones(f.zones,zoneMask(id));
 return f;
}
function paint(f) {
 const age=f.capturedAt?Date.now()-f.capturedAt:Infinity;
 const state=!f.capturedAt?(f.controller&&!f.heartbeat?'connecting':'offline'):age<3000?'live':age<15000?'delayed':'offline';
 f.tile.dataset.state=state;f.badge.textContent=state.toUpperCase();
 if(state!=='live'){f.rec.hidden=true;f.tile.classList.remove('recording');} // A stale tag is not a recording.
 const now=performance.now();while(f.arrivals.length&&now-f.arrivals[0]>3000)f.arrivals.shift();
 const size=f.img.naturalWidth?`${f.img.naturalWidth}×${f.img.naturalHeight}`:'';
 f.stats.textContent=state==='live'?[size,`${(f.arrivals.length/3).toFixed(1)} fps`].filter(Boolean).join(' · '):size;
 if(f.capturedAt)f.meta.textContent=`Frame #${f.seq} · ${age<2000?'just now':age<60000?`${Math.round(age/1000)} s ago`:new Date(f.capturedAt).toLocaleTimeString()}`;
}
function showFrame(f,bytes,head) {
 const url=URL.createObjectURL(new Blob([bytes],{type:'image/jpeg'}));const old=f.url;
 f.url=url;f.seq=/x-sequence:\s*(\d+)/i.exec(head)?.[1]||'?';f.capturedAt=Date.now()-Number(/x-age-ms:\s*(\d+)/i.exec(head)?.[1]||0);
 const motion=/x-motion:\s*(\d+)/i.exec(head)?.[1],event=Number(/x-event:\s*(\d+)/i.exec(head)?.[1]||0);
 f.level.style.width=motion===undefined?'0':`${Math.min(100,Number(motion)/3)}%`;f.level.parentElement.hidden=motion===undefined;
 f.rec.hidden=!event;f.rec.textContent=`REC #${event}`;f.tile.classList.toggle('recording',!!event);
 f.img.src=url;if(old)URL.revokeObjectURL(old);
 f.img.hidden=false;f.waiting.hidden=true;f.save.disabled=false;f.arrivals.push(performance.now());paint(f);
}
const headerEnd=b=>{for(let i=0;i+3<b.length;i++)if(b[i]===13&&b[i+1]===10&&b[i+2]===13&&b[i+3]===10)return i;return -1;};
async function readFeed(f,signal) {
 const r=await fetch(`/api/stream/${f.id}`,{headers:{Authorization:`Bearer ${token}`,'X-Protocol':'1'},cache:'no-store',signal});
 if(!r.ok||!r.body)throw Error(`Stream unavailable (${r.status}).`);
 const reader=r.body.getReader();let buf=new Uint8Array(0);
 try {
  for(;;) {
   let end;
   while((end=headerEnd(buf))>=0) {
    const head=new TextDecoder().decode(buf.subarray(0,end));const len=Number(/content-length:\s*(\d+)/i.exec(head)?.[1]);
    if(!Number.isSafeInteger(len)||len>131072)throw Error('Invalid stream part.');
    if(buf.length<end+4+len)break;
    const body=buf.slice(end+4,end+4+len);buf=buf.slice(end+4+len);
    if(/content-type:\s*image\/jpeg/i.test(head))showFrame(f,body,head);
    else if(!f.capturedAt){f.heartbeat=true;f.waiting.textContent='No live frames since the host started';paint(f);} // Idle heartbeat.
   }
   if(buf.length>8192&&headerEnd(buf)<0)throw Error('Invalid stream framing.');
   const {value,done}=await reader.read();if(done)return;
   const next=new Uint8Array(buf.length+value.length);next.set(buf);next.set(value,buf.length);buf=next;
  }
 } finally {reader.cancel().catch(()=>{});}
}
function startFeed(f) {
 if(f.controller||!token||document.hidden||tab!=='overview')return;
 clearTimeout(f.retry);const controller=new AbortController(),g=generation;f.controller=controller;paint(f);
 readFeed(f,controller.signal).catch(()=>{}).finally(()=>{
  if(f.controller!==controller)return; // Stopped deliberately.
  f.controller=null;paint(f);
  if(g===generation&&feeds.get(f.id)===f)f.retry=setTimeout(()=>startFeed(f),2000);
 });
}
function stopFeed(f){clearTimeout(f.retry);const c=f.controller;f.controller=null;c?.abort();}
function dropFeeds(){feeds.forEach(f=>{stopFeed(f);if(f.url)URL.revokeObjectURL(f.url);});feeds.clear();clearInterval(clockTimer);clockTimer=null;}
function syncFeeds(list) {
 const active=list.filter(d=>d.sequence>0).map(d=>d.id);
 for(const [id,f] of feeds)if(!active.includes(id)){stopFeed(f);if(f.url)URL.revokeObjectURL(f.url);feeds.delete(id);}
 for(const id of active){if(!feeds.has(id))feeds.set(id,feedTile(id));startFeed(feeds.get(id));}
 const tiles=[...feeds.values()].map(f=>f.tile);const shown=[...$('cameras').children];
 if(tiles.length!==shown.length||tiles.some((t,i)=>t!==shown[i]))$('cameras').replaceChildren(...(tiles.length?tiles:[text('p','No camera has sent a frame yet. Power on an ESP32-CAM or start a webcam source.','empty')]));
 $('idle-sources').replaceChildren(...list.filter(d=>!d.sequence).map(d=>text('span',`${d.id} · no frames yet`)));
 if(!clockTimer)clockTimer=setInterval(()=>feeds.forEach(paint),1000);
}
async function request(path,{key=token,method='GET',body,allowMissing=false,signal,timeoutMs=6000,headers:extra={}}={}) {
 const controller=new AbortController();inflight.add(controller);
 const timeout=setTimeout(()=>controller.abort(),timeoutMs);
 const abort=()=>controller.abort();signal?.addEventListener('abort',abort,{once:true});
 if(signal?.aborted) controller.abort();
 try {
  const headers={Authorization:`Bearer ${key}`,'X-Protocol':'1',...extra};
  if(body) headers['Content-Type']=body instanceof Blob?'image/jpeg':'text/plain';
  const r=await fetch(path,{method,body,headers,cache:'no-store',signal:controller.signal});
  if(allowMissing && r.status===404) return null;
  if(!r.ok) throw Error(({401:'Access denied. Check the credential for this action.',502:'AI answer unavailable (busy, over budget, or unknown outcome). Nothing was retried.',409:'Sequence conflict. Stop other sources using this device identity, then reconnect.',422:'Rule capacity reached. Edit an existing rule instead.',426:'Protocol version mismatch. Update this client.',429:'Upload budget reached. Wait, then restart snapshots.',503:'Storage unavailable. Operator recovery is required.'})[r.status] || `Request failed (${r.status}).`);
  return r;
 } finally {clearTimeout(timeout);inflight.delete(controller);signal?.removeEventListener('abort',abort);}
}
function stopCamera(note='Camera off. Tracks released.') {
 cameraEpoch++;clearTimeout(captureTimer);uploadController?.abort();uploadController=null;uploading=false;
 stream?.getTracks().forEach(t=>t.stop());stream=null;$('preview').srcObject=null;
 $('camera-preview').disabled=false;$('upload-start').disabled=true;$('source-device').disabled=false;
 message('camera-status',note);
}
function stopSpeech(note='Dictation stopped. Review or type your text.') {
 speechEpoch++;clearTimeout(speechTimer);const old=speech;speech=null;old?.abort();$('voice-start').disabled=false;message('voice-status',note);
}
function cancelDraft(note='Draft cancelled. Saved rules are unchanged.') {
 draft=null;$('rule-confirmation').hidden=true;$('confirm-check').checked=false;$('rule-save').disabled=true;message('rule-status',note);
}
function lock() {
 generation++;cancelProposal();clearTimeout(pollTimer);inflight.forEach(c=>c.abort());stopCamera();stopSpeech();cancelDraft();
 token='';operator='';devices=[];savedRules=[];dropFeeds();closePlayer();closeZones();stopVoice();seenNotices=null;speechSynthesis?.cancel();thumbs.forEach(URL.revokeObjectURL);thumbs.clear();activity=null;timelineKey='';seenAlerts=null;askEpoch++;
 for(const id of ['token','login-operator','ai-operator','ai-key','ask-input','source-key','operator-key','proposal-key','rule-intent','rule-json','routine-label','zones-concept']) $(id).value='';
 for(const id of ['cameras','idle-sources','incidents','events','rules-list','timeline','watch-list','alerts','ask-answer','toasts','routine-list','routine-status']) $(id).replaceChildren();
 $('alerts').hidden=true;$('ask-answer').hidden=true;
 $('speech-consent').checked=false;$('proposal-consent').checked=false;$('login').hidden=false;$('dashboard').hidden=true;$('lock').hidden=true;
 $('connect').disabled=false;message('connection','LOCKED');message('status','Session locked. Camera and microphone stopped.');
}
function showTab(next) {
 if(tab==='sources' && next!==tab) stopCamera('Camera stopped when leaving Sources.');
 if(tab==='rules' && next!==tab){stopSpeech();cancelProposal();}
 if(next!=='overview')feeds.forEach(stopFeed);
 tab=next;if(tab==='overview')feeds.forEach(startFeed);
 document.querySelectorAll('.view').forEach(e=>e.hidden=e.id!==`view-${tab}`);
 document.querySelectorAll('[data-tab]').forEach(e=>e.setAttribute('aria-selected',String(e.dataset.tab===tab)));
}
function populate(select, values) {
 const current=select.value;select.replaceChildren(...values.map(v=>{const o=text('option',v);o.value=v;return o;}));
 if(values.includes(current))select.value=current;
}
function renderRules(rules) {
 const rows=rules.map(r=>{const row=text('article','', 'list-row');const content=text('div','');content.append(text('strong',r.id),text('p',`${r.device} · ${r.signal} ≥ ${r.threshold} · ${r.cooldown}s cooldown · ${r.enabled?'Active':'Disabled'}`));
 const edit=text('button','Edit / disable');edit.onclick=()=>{setEditor(r);showTab('rules');message('rule-status','Edit the rule, then review and explicitly confirm your change.');};row.append(content,edit);return row;});
 return rows.length?rows:[text('p','No rules yet. A draft does nothing until you confirm it.','empty')];
}
async function refresh() {
 clearTimeout(pollTimer);if(!token)return;const g=generation,run=++refreshEpoch;
 $('refresh').disabled=true;
 try {
  const [list,signals,rules,events,provider,act,watches]=await Promise.all(['/api/devices','/api/security','/api/rules','/api/events','/api/rule-provider','/api/activity','/api/watches'].map(async p=>(await request(p)).json()));
  const incidents=[];
  for(const d of list) {
   const response=await request(`/api/review/${d.id}`,{allowMissing:true});
   if(response) {
    const report=await response.json();const item=text('article','', 'incident');
    item.append(text('strong',report.source==='meta'?'META AI · ADVISORY':'SIMULATED AI · NO PROVIDER CALL','ai-source'),text('h3',d.id),text('p',report.assessment?.incident_summary || 'Invalid report'),text('p',`Local policy: ${report.policy?.deterministic_recommendation || 'unknown'} · Human approval required · No actions executed`),text('p',`Evidence sequence ${report.evidence?.sequence ?? '?'} · SHA-256 ${(report.evidence?.image_sha256||'').slice(0,12)}`,'muted'),text('p',`Reviewed ${new Date(report.created_unix*1000).toLocaleString()}. May predate the latest snapshot.`,'muted'));incidents.push(item);
   }
  }
  if(g!==generation||run!==refreshEpoch)return;
  message('rule-provider',provider.source==='meta'?'META · LIVE CLOUD (explicit request only)':provider.source==='mock'?'SIMULATED · NO PROVIDER CALL':'AI DISABLED · LOCAL TEMPLATE available');$('rule-ai').disabled=provider.source==='disabled';devices=list;savedRules=rules;for(const id of ['source-device','rule-device','ask-device','watch-device','routine-device'])populate($(id),list.map(d=>d.id));
  if(!$('ask-device').dataset.touched){const best=[...list].sort((a,b)=>b.sequence-a.sequence)[0];if(best)$('ask-device').value=best.id;}
  renderActivity(act,watches);
  syncFeeds(list);$('incidents').replaceChildren(...(incidents.length?incidents:[text('p','No incident review yet. Run the synthetic incident demo or an explicitly authorized Meta review. No automatic cloud call runs here.','empty')]));
  $('rules-list').replaceChildren(...renderRules(rules));
  $('events').replaceChildren(...(events.length?events.map(e=>{const row=text('article','', 'list-row');row.append(text('strong',e.rule),text('p',`${e.device} · ${e.signal}: ${e.value} · ${new Date(e.at*1000).toLocaleTimeString()} · Panel event only`));return row;}):[text('p','No local rule events. Rules evaluate once per second on the host, even with the dashboard closed.','empty')]));
  message('count',String(list.length));message('auth-count',String(signals.auth_failures));message('replay-count',String(signals.devices.reduce((n,d)=>n+d.replays,0)));message('rule-count',String(rules.filter(r=>r.enabled).length));
  message('security-signals','Live from your host. Snapshots are kept once per second; counters cover this server session.');
  $('login').hidden=true;$('dashboard').hidden=false;$('lock').hidden=false;message('connection','CONNECTED');message('status',`Status updated ${new Date().toLocaleTimeString()} · Video is live; counters refresh every 5 seconds.`);
 } catch(e) {if(g!==generation||run!==refreshEpoch)return;message('connection','STALE / UNAVAILABLE');message('status',`${e.name==='AbortError'?'Request interrupted or timed out.':e.message} Previously displayed data may be stale.`);}
 finally {if(g===generation&&run===refreshEpoch){$('connect').disabled=false;$('refresh').disabled=false;if(token)pollTimer=setTimeout(refresh,5000);}}
}
$('connect').onclick=()=>{generation++;clearTimeout(pollTimer);token=$('token').value.trim();$('token').value='';
 const op=$('login-operator').value.trim();$('login-operator').value='';operator=/^[a-fA-F0-9]{64}$/.test(op)?op:'';if(!/^[a-fA-F0-9]{64}$/.test(token)){message('status','Enter a 64-character hexadecimal viewer token.');token='';return;}$('connect').disabled=true;refresh();};
$('token').onkeydown=e=>{if(e.key==='Enter'&&!$('connect').disabled)$('connect').click();};
$('lock').onclick=lock;$('refresh').onclick=refresh;
for(const e of document.querySelectorAll('[data-tab]'))e.onclick=()=>showTab(e.dataset.tab);
for(const b of document.querySelectorAll('#ask-suggestions button'))b.onclick=()=>{$('ask-input').value=b.dataset.q;$('ask-send').click();};
$('camera-preview').onclick=async()=>{
 stopCamera();const epoch=cameraEpoch;const session=generation;
 if(!navigator.mediaDevices?.getUserMedia){message('camera-status','Camera access needs a supported browser on localhost or HTTPS.');return;}
 $('camera-preview').disabled=true;message('camera-status','Waiting for camera permission. No upload is running.');
 try{
  const selected=$('camera-select').value;
  const incoming=await navigator.mediaDevices.getUserMedia({audio:false,video:{width:{ideal:320},height:{ideal:240},...(selected?{deviceId:{exact:selected}}:{})}});
  if(epoch!==cameraEpoch||session!==generation||tab!=='sources'||document.hidden){incoming.getTracks().forEach(t=>t.stop());return;}
  stream=incoming;for(const t of stream.getTracks())t.onended=()=>stopCamera('Camera disconnected. Tracks released.');
  $('preview').srcObject=stream;await $('preview').play();
  if(epoch!==cameraEpoch)return;
  const available=(await navigator.mediaDevices.enumerateDevices()).filter(d=>d.kind==='videoinput');
  if(epoch!==cameraEpoch)return;
  const current=stream.getVideoTracks()[0]?.getSettings().deviceId;
  $('camera-select').replaceChildren(...available.map((d,i)=>{const o=text('option',d.label||`Camera ${i+1}`);o.value=d.deviceId;return o;}));if(current)$('camera-select').value=current;
  $('upload-start').disabled=false;message('camera-status','Preview only. Start snapshots to send JPEGs to your local host.');
 }catch(e){if(epoch===cameraEpoch)stopCamera(`Camera unavailable: ${e.name}. You can retry or use the synthetic simulator.`);}
};
$('camera-stop').onclick=()=>{stopCamera();$('source-key').value='';};
$('camera-select').onchange=()=>stopCamera('Camera selection changed. Open a new preview.');
$('source-device').onchange=()=>stopCamera('Device identity changed. Open a new preview.');
$('upload-start').onclick=async()=>{
 if(!stream||uploading)return;
 const id=$('source-device').value,key=$('source-key').value.trim(),epoch=cameraEpoch,session=generation;
 if(!/^[a-fA-F0-9]{64}$/.test(key)){message('camera-status','Enter this configured device’s upload token.');return;}
 uploading=true;webcamBackground=null;$('upload-start').disabled=true;$('source-device').disabled=true;uploadController=new AbortController();const signal=uploadController.signal;
 try{
  const state=await(await request(`/api/device-state/${id}`,{key,signal})).json();let next=BigInt(state.next_sequence);
  async function capture(){
   if(epoch!==cameraEpoch||session!==generation||!stream||!uploading)return;
   const canvas=document.createElement('canvas');canvas.width=320;canvas.height=240;const started=performance.now();
   try{
    if($('preview').readyState<2)throw Error('No camera frame available.');
    canvas.getContext('2d').drawImage($('preview'),0,0,320,240);
    const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/jpeg',.75));
    if(epoch!==cameraEpoch||session!==generation||!stream)return;
    if(!blob||blob.size>131072)throw Error('JPEG exceeds the 128 KiB limit. Capture stopped.');
    const seq=next++;const controller=new AbortController();uploadController=controller;const timeout=setTimeout(()=>controller.abort(),6000);
    const motion=webcamMotion(canvas);
    try{const r=await fetch(`/api/frame/${id}`,{method:'POST',headers:{Authorization:`Bearer ${key}`,'Content-Type':'image/jpeg','X-Sequence':seq.toString(),'X-Protocol':'1','X-Motion':String(motion)},body:blob,signal:controller.signal});
     if(!r.ok)throw Error(r.status===409?'Sequence conflict: another source may share this identity.':r.status===429?'Rate limited. Wait before reconnecting.':`Upload rejected (${r.status}).`);
     const mask=r.headers.get('X-Motion-Mask');if(mask&&/^[0-9a-f]{12}$/.test(mask))webcamMask=BigInt('0x'+mask);
    }finally{clearTimeout(timeout);}
    if(epoch!==cameraEpoch||session!==generation)return;
    message('camera-status',`Uploading · sequence ${seq} · ${blob.size} bytes · motion ${(motion/10).toFixed(1)}%`);captureTimer=setTimeout(capture,Math.max(0,200-(performance.now()-started)%200));
   }catch(e){if(epoch===cameraEpoch)stopCamera(`${e.name==='AbortError'?'Upload outcome unknown after interruption.':e.message} Reconnect to read the persisted sequence; no automatic retry.`);}
  }
  if(epoch!==cameraEpoch||session!==generation)return;await capture();
 }catch(e){if(epoch===cameraEpoch)stopCamera(`Cannot start snapshots: ${e.message}`);}
};
function validateRule(r){
 const keys=['action','cooldown','device','enabled','id','signal','threshold'];
 if(!r||typeof r!=='object'||Array.isArray(r)||JSON.stringify(Object.keys(r).sort())!==JSON.stringify(keys))throw Error('Draft must contain exactly the seven supported schema fields.');
 if(typeof r.id!=='string'||typeof r.device!=='string'||typeof r.signal!=='string'||!/^[a-z0-9-]{1,24}$/.test(r.id)||!devices.some(d=>d.id===r.device)||!['replays','invalid_frames','auth_failures','stale_seconds'].includes(r.signal)||!Number.isInteger(r.threshold)||r.threshold<1||r.threshold>3600||!Number.isInteger(r.cooldown)||r.cooldown<10||r.cooldown>86400||typeof r.enabled!=='boolean'||r.action!=='ui_event')throw Error('Invalid rule. Only configured devices, bounded thresholds and panel events are supported.');return {...r};
}
function editorValue(){return validateRule({id:$('rule-id').value.trim(),device:$('rule-device').value,signal:$('rule-signal').value,threshold:Number($('rule-threshold').value),cooldown:Number($('rule-cooldown').value),enabled:$('rule-enabled').checked,action:'ui_event'});}
function setEditor(r){cancelProposal();validateRule(r);cancelDraft('Draft loaded. Edit, review, then confirm.');for(const k of ['id','device','signal','threshold','cooldown'])$(`rule-${k}`).value=r[k];$('rule-enabled').checked=r.enabled;}
for(const e of document.querySelectorAll('#rule-editor input,#rule-editor select'))e.oninput=()=>{cancelProposal();cancelDraft('Draft changed. Review it again before confirming.');};
$('rule-parse').onclick=()=>{try{
 let intent=$('rule-intent').value.trim().toLowerCase().replace(/[.!]$/,'');const words=['zero','one','two','three','four','five','six','seven','eight','nine','ten'];intent=intent.replace(/\b(zero|one|two|three|four|five|six|seven|eight|nine|ten)\b/g,v=>String(words.indexOf(v)));
 const count=intent.match(/^when ([a-z0-9-]{1,24}) has (\d+) (replays|invalid frames|authentication failures),? notify me$/);
 const stale=intent.match(/^when ([a-z0-9-]{1,24}) is stale for (\d+) seconds,? notify me$/);const match=count||stale;
 if(!match)throw Error('Unsupported phrase. Use a shown template or the structured editor. Nothing was activated.');
 const signal=stale?'stale_seconds':({'replays':'replays','invalid frames':'invalid_frames','authentication failures':'auth_failures'})[count[3]];
 setEditor({id:'local-review',device:match[1],signal,threshold:Number(match[2]),cooldown:60,enabled:true,action:'ui_event'});
 message('rule-status','LOCAL TEMPLATE draft. Review before confirming.');
 }catch(e){cancelDraft(e.message);}};
$('rule-import').onclick=()=>{try{const raw=$('rule-json').value;if(raw.length>1024||raw.includes('\\'))throw Error('Use a bounded JSON draft with literal field names and no escapes.');const r=JSON.parse(raw); // Schema validation rejects code/actions; no eval.
 // Reject duplicate property names rather than accepting ambiguous model output.
 const keys=[...raw.matchAll(/"([a-z_]+)"\s*:/g)].map(m=>m[1]);if(new Set(keys).size!==keys.length)throw Error('Duplicate fields are not allowed.');setEditor(validateRule(r));}catch(e){cancelDraft(`Draft rejected: ${e.message}`);}};
$('rule-review').onclick=()=>{try{draft=Object.freeze(editorValue());$('rule-summary').textContent=`${draft.enabled?'ACTIVATE / UPDATE':'DISABLE'} ${draft.id}\nDevice: ${draft.device}\nSignal: ${draft.signal} ≥ ${draft.threshold}\nCooldown: ${draft.cooldown} seconds\nAction: create a local panel event only\n${savedRules.some(r=>r.id===draft.id)?'This replaces the existing rule with this ID.':'This creates a new rule.'}\nCounters reset at host restart. No model or external message is involved.`;$('confirm-check').checked=false;$('rule-save').disabled=true;$('rule-confirmation').hidden=false;message('rule-status','Review the exact fields above. Activation requires the checkbox, operator token and Confirm.');}catch(e){cancelDraft(e.message);}};
$('confirm-check').onchange=()=>{$('rule-save').disabled=!draft||!$('confirm-check').checked;};
$('rule-cancel').onclick=()=>cancelDraft();
$('rule-save').onclick=async()=>{
 if(!draft||!$('confirm-check').checked)return;const value=draft,key=$('operator-key').value.trim(),g=generation;
 if(!/^[a-fA-F0-9]{64}$/.test(key)){message('rule-status','Enter the separate operator token to save a rule.');return;}
 $('rule-save').disabled=true;message('rule-status','Saving the confirmed rule…');
 try{await request(`/api/rules/${value.id}`,{method:'POST',key,body:`device=${value.device}\nsignal=${value.signal}\nthreshold=${value.threshold}\ncooldown=${value.cooldown}\nenabled=${value.enabled?1:0}\n`});if(g!==generation)return;cancelDraft(`Saved ${value.id}. ${value.enabled?'Local event evaluation is active.':'Rule disabled.'}`);refresh();}
 catch(e){if(g===generation)cancelDraft(`${e.message} If interrupted, refresh saved rules to check the outcome before retrying.`);}
};
$('voice-start').onclick=()=>{
 cancelProposal();
 if(!$('speech-consent').checked){message('voice-status','Consent is required before browser recognition can receive audio. Typed input remains available.');return;}
 const Recognition=window.SpeechRecognition||window.webkitSpeechRecognition;if(!Recognition){message('voice-status','Speech recognition is unavailable in this browser. Type the rule instead.');return;}
 stopSpeech();cancelDraft('Dictation only fills the text field. Review is still required.');const epoch=speechEpoch,g=generation;
 try{const r=new Recognition();speech=r;r.lang='en-US';r.continuous=false;r.interimResults=false;$('voice-start').disabled=true;
 r.onresult=e=>{if(epoch!==speechEpoch||g!==generation)return;const transcript=Array.from(e.results).map(v=>v[0].transcript).join(' ').slice(0,240);$('rule-intent').value=transcript;message('voice-status','Transcript ready. Correct it, then create a draft. Nothing is active.');};
 r.onerror=e=>{if(epoch===speechEpoch)stopSpeech(`Recognition failed (${e.error}). Use typed input or retry.`);};
 r.onend=()=>{if(epoch===speechEpoch){speech=null;clearTimeout(speechTimer);$('voice-start').disabled=false;}};
 r.start();message('voice-status','Listening via your browser’s recognition service. Stop at any time.');speechTimer=setTimeout(()=>stopSpeech('Dictation time limit reached. Review or type your text.'),20000);
 }catch(e){stopSpeech('Could not start speech recognition. Use typed input.');}
};
$('voice-stop').onclick=()=>stopSpeech();
window.addEventListener('pagehide',lock);
document.addEventListener('visibilitychange',()=>{if(document.hidden){stopCamera('Camera stopped while the page was hidden.');stopSpeech();feeds.forEach(stopFeed);}else feeds.forEach(startFeed);});

function cancelProposal(){proposalEpoch++;proposalController?.abort();proposalController=null;message('proposal-status','Proposal cancelled or inactive. A provider request already sent may still complete and consume budget. No automatic retry.');}
$('proposal-cancel').onclick=()=>{cancelProposal();cancelDraft();};
$('rule-intent').oninput=()=>{cancelProposal();cancelDraft('Text changed. Request a new proposal or local template.');};
$('rule-ai').onclick=async()=>{
 cancelProposal();cancelDraft('Waiting for advisory interpretation. No rule activated.');
 const key=$('proposal-key').value.trim(),intent=$('rule-intent').value.trim(),id=$('rule-device').value;
 if(!$('proposal-consent').checked||!intent||intent.length>240||!/^[a-fA-F0-9]{64}$/.test(key)){message('proposal-status','Enter text and an operator token, then approve the provider request.');return;}
 const epoch=proposalEpoch,g=generation;proposalController=new AbortController();
 message('proposal-status','Interpreting… no rule will be activated.');
 try{
  const r=await(await request(`/api/rule-draft/${id}`,{key,method:'POST',body:intent,signal:proposalController.signal,timeoutMs:28000})).json();
  if(epoch!==proposalEpoch||g!==generation)return;
  if(!['meta','mock'].includes(r.source)||typeof r.explanation!=='string'||r.explanation.length>240)throw Error('Invalid proposal response.');
  if(r.rule){setEditor(validateRule(r.rule));message('rule-status','AI proposal only. Check the interpretation, edit fields, review, then confirm separately.');}
  else cancelDraft('Unsupported request. Nothing was approximated or activated.');
  message('proposal-status',`${r.source==='meta'?'META · LIVE':'SIMULATED · NOT TEXT INTERPRETATION'}: ${r.explanation}`);
 }catch(e){if(epoch===proposalEpoch&&g===generation){cancelDraft();message('proposal-status',`${e.message} Provider outcome may be unknown. Check budget/provider records before retrying. No rule activated.`);}}
};

$('proposal-consent').onchange=()=>{if(!$('proposal-consent').checked)cancelProposal();};

// ---- Edge-style motion for the webcam source: same 40x30 background model as the ESP32 firmware.
let webcamBackground=null,webcamMask=0n;
function webcamMotion(source) {
 const c=document.createElement('canvas');c.width=40;c.height=30;const g=c.getContext('2d',{willReadFrequently:true});
 g.drawImage(source,0,0,40,30);const px=g.getImageData(0,0,40,30).data,n=1200,luma=new Float32Array(n);let shift=0;
 for(let i=0;i<n;i++){luma[i]=(77*px[4*i]+150*px[4*i+1]+29*px[4*i+2])/256;if(webcamBackground)shift+=luma[i]-webcamBackground[i];}
 if(!webcamBackground){webcamBackground=luma;return 0;}
 shift/=n;let changed=0,watched=0;
 for(let i=0;i<n;i++){const d=luma[i]-webcamBackground[i]-shift,zone=Math.floor(Math.floor(i/40)*6/30)*8+Math.floor((i%40)*8/40);
  if(!((webcamMask>>BigInt(zone))&1n)){watched++;if(Math.abs(d)>18)changed++;}webcamBackground[i]+=(luma[i]-webcamBackground[i])/8;}
 return watched?Math.round(changed*1000/watched):0;
}

// ---- Operator actions, toasts and alerts.
const operatorToken=()=>{const v=$('ai-operator').value.trim();return /^[a-fA-F0-9]{64}$/.test(v)?v:operator;};
const needOperator=id=>{message(id,'Enter the operator token (at unlock, or in Watches & rules → Meta AI).');};
const plain=s=>s.replace(/[\u0000-\u001f]/g,' ');
function toast(title,body,kind=''){
 const t=text('div','',`toast ${kind}`);t.append(text('strong',title),text('p',body));$('toasts').append(t);
 if($('speak-alerts').checked)say(`${title.replace(/^[⚠✓]\s*/,'')}. ${body}`);
 setTimeout(()=>t.remove(),9000);
 if(window.Notification?.permission==='granted'&&document.hidden){try{new Notification(title,{body});}catch{}}
}
$('notify-enable').onclick=async()=>{
 if(!window.Notification){message('status','This browser does not support notifications. Alerts still appear here.');return;}
 const p=await Notification.requestPermission();message('status',p==='granted'?'Alerts will also notify you while this tab is in the background.':'Notifications blocked. Alerts still appear on this page.');
};
const when=unix=>{const d=new Date(unix*1000),today=new Date().toDateString()===d.toDateString();return today?d.toLocaleTimeString([], {hour:'2-digit',minute:'2-digit',second:'2-digit'}):d.toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});};
const duration=e=>`${Math.max(1,(e.end??Math.floor(Date.now()/1000))-e.start)} s`;
const sourceLabel=s=>s==='meta'?'META AI':'SIMULATED';

// ---- Activity: AI status, alert banner, timeline and watches.
function renderActivity(act,watches){
 activity={...act,watches};const ai=act.ai,byId=new Map(watches.map(w=>[w.id,w]));
 feedback=new Map((act.feedback||[]).map(x=>[`${x.event}:${x.watch}`,x.right]));
 renderHistogram(act.events);renderCare(act);
 for(const [id,f] of feeds)paintZones(f.zones,zoneMask(id));
 const status=ai.mode==='meta'?`META · ${ai.model} · ${ai.used}/${ai.daily} calls today`:ai.mode==='mock'?'SIMULATION · add a Meta key to see real descriptions':'AI DISABLED by server configuration';
 message('ai-status',status);message('ai-chip',ai.mode==='meta'?`META · ${ai.model}`:ai.mode==='mock'?'SIMULATED AI':'AI OFF');
 $('ai-chip').classList.toggle('live',ai.mode==='meta');$('ai-card').classList.toggle('live',ai.mode==='meta');
 if(!$('ai-language').dataset.touched)$('ai-language').value=ai.language;if(!$('ai-auto').dataset.touched)$('ai-auto').checked=ai.auto;
 // Alerts: everything from the last 10 minutes in the banner; only new ones toast.
 const alerts=act.events.flatMap(e=>(e.ai?.alerts||[]).map(a=>({...a,event:e,key:`${e.id}:${a.watch}`})));
 if(seenAlerts)for(const a of alerts)if(!seenAlerts.has(a.key))toast(`⚠ ${byId.get(a.watch)?.prompt||a.watch}`,`${a.event.device} · ${a.reason}`);
 seenAlerts=new Set(alerts.map(a=>a.key));
 const notices=act.notices||[];
 if(seenNotices)for(const n of notices)if(!seenNotices.has(n.id))n.kind==='missed'?toast(`⚠ No activity: ${n.label}`,`${n.device} · nothing moved during the expected window`,'missed'):toast(`✓ ${n.label}`,`${n.device} · first motion at ${clock(n.at)}`,'seen');
 seenNotices=new Set(notices.map(n=>n.id));
 // A dismissed false alarm leaves the banner.
 const recent=alerts.filter(a=>Date.now()/1000-a.event.start<600&&feedback.get(a.key)!==false);
 const missed=notices.filter(n=>n.kind==='missed'&&Date.now()/1000-n.at<12*3600);
 $('alerts').hidden=!recent.length&&!missed.length;
 $('alerts').replaceChildren(...missed.slice(0,2).map(n=>{const row=text('button','','alert missed');row.append(text('strong',`⚠ No activity: ${n.label}`),text('span',`${n.device} · checked at ${clock(n.at)} · nothing moved in the expected window`));row.onclick=()=>showTab('activity');return row;}),
  ...recent.slice(0,4).map(a=>{const row=text('button','','alert');row.append(text('strong',`⚠ ${byId.get(a.watch)?.prompt||a.watch}`),text('span',`${a.event.device} · ${when(a.event.start)} · ${a.reason} (${a.confidence})`));row.onclick=()=>openPlayer(a.event);return row;}));
 const total=act.events.length,described=act.events.filter(e=>e.ai).length;
 message('activity-summary',total?`${total} motion events · ${described} described by AI · clips include the seconds before motion started.`:'No motion events yet. Cameras report motion detected on the device itself; events record a clip with pre-roll.');
 const key=JSON.stringify([act.events,act.feedback,watches]);
 if(key!==timelineKey){timelineKey=key;renderTimeline(act.events,byId);}
 $('watch-list').replaceChildren(...(watches.length?watches.map(w=>watchRow(w)):[text('p','No watches yet. Example: “A person at the front door” or “The garage door left open”.','empty')]));
}
function renderTimeline(events,byId){
 const rows=events.map(e=>{
  const card=text('article','','event');card.classList.toggle('alerted',!!e.ai?.alerts?.length);
  const thumb=text('button','','thumb');thumb.setAttribute('aria-label',`Play event ${e.id}`);thumb.onclick=()=>openPlayer(e);
  const img=document.createElement('img');img.alt='';thumb.append(img,text('span',duration(e),'thumb-time'));
  if(e.end!==null)loadThumb(e.id,img);else thumb.append(text('span','● REC','thumb-rec'));
  const body=text('div','','event-body');
  const head=text('div','','event-head');head.append(text('strong',`${e.device}`),text('span',when(e.start),'muted'),text('span',`motion ${(e.peak/10).toFixed(1)}%`,'muted'));
  if(e.ai?.activity&&e.ai.activity!=='none')head.append(text('span',e.ai.activity.replaceAll('_',' '),'chip'));
  body.append(head);
  if(e.ai){body.append(text('p',e.ai.caption,'caption'));
   for(const a of e.ai.alerts)body.append(alertRow(e,a,byId));
   body.append(text('span',sourceLabel(e.ai.source),`source ${e.ai.source}`));}
  else body.append(text('p',e.pending?'Describing with AI…':e.failed?'AI description unavailable (disabled, busy or over budget).':e.end===null?'Recording…':'Not described (rate limited to one automatic analysis per camera every 10 s).','caption muted'));
  const actions=text('div','','button-row');const play=text('button','Play clip');play.onclick=()=>openPlayer(e);actions.append(play);
  if(e.end!==null&&!e.pending){const again=text('button',e.ai?'Re-analyze':'Analyze with AI');again.onclick=()=>analyzeEvent(e.id,again);actions.append(again);}
  const teach=teachControl(e,byId);if(teach)actions.append(teach);
  body.append(actions);card.append(thumb,body);return card;
 });
 $('timeline').replaceChildren(...(rows.length?rows:[text('p','Wave at a camera: motion detected on the device creates an event with a clip here.','empty')]));
}
async function loadThumb(id,img){
 if(thumbs.has(id)){img.src=thumbs.get(id);return;}
 try{const r=await request(`/api/keyframe/${id}`,{allowMissing:true});if(!r||!token)return;const url=URL.createObjectURL(await r.blob());
  if(thumbs.has(id)){URL.revokeObjectURL(url);img.src=thumbs.get(id);return;}thumbs.set(id,url);img.src=url;
  if(thumbs.size>150){const [old,u]=thumbs.entries().next().value;URL.revokeObjectURL(u);thumbs.delete(old);}}catch{}
}
async function analyzeEvent(id,button){
 const key=operatorToken();if(!key){needOperator('status');return;}
 button.disabled=true;
 try{await request(`/api/analyze/${id}`,{key,method:'POST',body:'analyze'});message('status',`Event #${id} sent to the vision model.`);refresh();}
 catch(e){button.disabled=false;message('status',e.message);}
}

// ---- Clip player: "XLC1" then per frame u32 LE offset_ms, u32 LE length, JPEG.
function closePlayer(){if(!player)return;clearTimeout(player.timer);player.urls.forEach(URL.revokeObjectURL);player=null;$('player').hidden=true;$('player-img').removeAttribute('src');}
async function openPlayer(e){
 closePlayer();const session=generation;const me={urls:[],offsets:[],index:0,playing:true,timer:null};player=me;
 $('player').hidden=false;message('player-title',`${e.device} · ${when(e.start)} · ${duration(e)}`);
 message('player-caption',e.ai?`${sourceLabel(e.ai.source)}: ${e.ai.caption}`:'Loading clip…');
 try{
  const buf=new Uint8Array(await(await request(`/api/clip/${e.id}`,{timeoutMs:20000})).arrayBuffer());
  if(player!==me||session!==generation)return;
  const view=new DataView(buf.buffer);if(String.fromCharCode(...buf.subarray(0,4))!=='XLC1')throw Error('Invalid clip.');
  for(let at=4;at+8<=buf.length;){const off=view.getUint32(at,true),len=view.getUint32(at+4,true);if(at+8+len>buf.length)break;
   me.urls.push(URL.createObjectURL(new Blob([buf.subarray(at+8,at+8+len)],{type:'image/jpeg'})));me.offsets.push(off);at+=8+len;}
  if(!me.urls.length)throw Error('Empty clip.');
  $('player-seek').max=String(me.urls.length-1);if(!e.ai)message('player-caption',`${me.urls.length} frames · motion peak ${(e.peak/10).toFixed(1)}%`);
  showClipFrame(0);if(me.playing)scheduleClip();
 }catch(err){if(player===me)message('player-caption',`Clip unavailable: ${err.message}`);}
}
function showClipFrame(i){if(!player)return;player.index=i;$('player-img').src=player.urls[i];$('player-seek').value=String(i);message('player-time',`${((player.offsets[i]-player.offsets[0])/1000).toFixed(1)} s · frame ${i+1}/${player.urls.length}`);}
function scheduleClip(){
 if(!player)return;clearTimeout(player.timer);const p=player;
 if(p.index>=p.urls.length-1){p.playing=false;message('player-toggle','Replay');return;}
 message('player-toggle','Pause');
 p.timer=setTimeout(()=>{if(player!==p)return;showClipFrame(p.index+1);scheduleClip();},Math.min(1000,Math.max(30,p.offsets[p.index+1]-p.offsets[p.index])));
}
$('player-toggle').onclick=()=>{if(!player)return;if(player.playing){player.playing=false;clearTimeout(player.timer);message('player-toggle','Play');}else{if(player.index>=player.urls.length-1)showClipFrame(0);player.playing=true;scheduleClip();}};
$('player-seek').oninput=()=>{if(!player)return;player.playing=false;clearTimeout(player.timer);message('player-toggle','Play');showClipFrame(Number($('player-seek').value));};
$('player-close').onclick=closePlayer;
$('player').onclick=e=>{if(e.target===$('player'))closePlayer();};
document.addEventListener('keydown',e=>{if(e.key==='Escape')closePlayer();});

// ---- Ask your cameras: current frame + event log, answered by the vision model.
$('ask-device').onchange=()=>{$('ask-device').dataset.touched='1';};
$('ask-input').onkeydown=e=>{if(e.key==='Enter'&&!$('ask-send').disabled)$('ask-send').click();};
$('ask-send').onclick=async()=>{
 const spokenQuestion=askSpoken;askSpoken=false;
 const question=$('ask-input').value.trim(),device=$('ask-device').value,key=operatorToken(),epoch=++askEpoch,g=generation;
 const box=$('ask-answer');box.hidden=false;
 if(!question||!device){box.replaceChildren(text('p','Type a question and pick a camera.'));return;}
 if(!key){box.replaceChildren(text('p','Asking uses AI budget, so it needs the operator token (at unlock, or in Watches & rules → Meta AI).'));return;}
 $('ask-send').disabled=true;box.replaceChildren(text('p',`Looking at ${device} and its recent events…`,'thinking'));
 try{
  const r=await(await request(`/api/ask/${device}`,{key,method:'POST',body:question,timeoutMs:35000,headers:{'X-Utc-Offset':String(-new Date().getTimezoneOffset())}})).json();
  if(epoch!==askEpoch||g!==generation)return;
  if(!['meta','mock'].includes(r.source)||typeof r.answer!=='string'||!Array.isArray(r.events))throw Error('Invalid answer.');
  const head=text('div','','ask-head');head.append(text('span',sourceLabel(r.source),`source ${r.source}`),text('span',`“${plain(question)}”`,'muted'));
  const cites=text('div','','button-row');
  for(const id of r.events){const e=activity?.events.find(x=>x.id===id);if(!e)continue;const b=text('button',`▶ #${id} · ${e.device} · ${when(e.start)}`,'cite');b.onclick=()=>openPlayer(e);cites.append(b);}
  if(typeof r.watch==='string'&&r.watch.length>=3){
   const make=text('button',`＋ Create watch: “${plain(r.watch)}”`,'primary');
   make.onclick=async()=>{const ids=new Set((activity?.watches||[]).map(w=>w.id));let n=1;while(ids.has(`watch-${n}`))n++;
    make.disabled=true;await saveWatch({id:`watch-${n}`,device,prompt:r.watch.slice(0,200),cooldown:120,enabled:true});message('status',$('watch-status').textContent);make.textContent='✓ Watch created · checked on every motion event';};
   cites.append(make);
  }
  box.replaceChildren(head,text('p',r.answer,'answer'),cites);
  if(spokenQuestion||$('speak-alerts').checked)say(r.answer);
 }catch(e){if(epoch===askEpoch)box.replaceChildren(text('p',e.name==='AbortError'?'No answer within 35 s. The outcome is unknown; nothing was retried.':e.message));}
 finally{if(epoch===askEpoch)$('ask-send').disabled=false;}
};

// ---- Visual watches.
function watchRow(w){
 const row=text('article','','list-row');const content=text('div','');
 content.append(text('strong',w.prompt),text('p',`${w.id} · ${w.device} · alert at most every ${w.cooldown} s · ${w.enabled?'Active':'Paused'}${w.learned?` · learned from ${w.learned} label${w.learned===1?'':'s'}`:''}`));
 const actions=text('div','','button-row');
 const toggle=text('button',w.enabled?'Pause':'Resume');toggle.onclick=()=>saveWatch({...w,enabled:!w.enabled});
 const edit=text('button','Edit');edit.onclick=()=>{$('watch-id').value=w.id;$('watch-device').value=w.device;$('watch-prompt').value=w.prompt;$('watch-cooldown').value=String(w.cooldown);message('watch-status',`Editing ${w.id}. Save to apply.`);};
 const del=text('button','Delete');del.onclick=()=>deleteWatch(w.id);
 actions.append(toggle,edit,del);row.append(content,actions);return row;
}
async function saveWatch(w){
 const key=operatorToken();if(!key){needOperator('watch-status');return;}
 if(!/^[a-z0-9-]{1,24}$/.test(w.id)||w.prompt.length<3||w.prompt.length>200||/[\u0000-\u001f]/.test(w.prompt)||!(w.cooldown>=30&&w.cooldown<=86400)){message('watch-status','Use a lowercase ID, a 3–200 character description on one line, and a 30–86400 s cooldown.');return;}
 try{await request(`/api/watches/${w.id}`,{key,method:'POST',body:`device=${w.device}\nprompt=${w.prompt}\ncooldown=${w.cooldown}\nenabled=${w.enabled?1:0}`});
  message('watch-status',`Saved ${w.id}. It is checked on the next motion event from ${w.device}.`);refresh();}
 catch(e){message('watch-status',e.message.includes('422')?'Watch limit reached (8). Delete one first.':e.message);}
}
async function deleteWatch(id){
 const key=operatorToken();if(!key){needOperator('watch-status');return;}
 try{await request(`/api/watches/${id}`,{key,method:'POST',body:'delete=1'});message('watch-status',`Deleted ${id}.`);refresh();}catch(e){message('watch-status',e.message);}
}
$('watch-save').onclick=()=>saveWatch({id:$('watch-id').value.trim(),device:$('watch-device').value,prompt:$('watch-prompt').value.trim(),cooldown:Number($('watch-cooldown').value),enabled:true});

// ---- Meta AI settings: the key is write-only from the browser.
async function aiPost(path,body,done){
 const key=operatorToken();if(!key){needOperator('ai-message');return;}
 try{await request(path,{key,method:'POST',body});message('ai-message',done);refresh();}catch(e){message('ai-message',e.message);}
}
$('ai-key-save').onclick=()=>{const v=$('ai-key').value.trim();$('ai-key').value='';if(v.length<8||/\s/.test(v)){message('ai-message','Paste the full Meta API key.');return;}aiPost('/api/ai-key',v,'Key saved on the host. Live Meta vision is on for new events and questions.');};
$('ai-key-remove').onclick=()=>aiPost('/api/ai-key',' ','Key removed from the host. AI is back in labeled simulation.');
$('ai-language').onchange=()=>{$('ai-language').dataset.touched='1';};
$('ai-auto').onchange=()=>{$('ai-auto').dataset.touched='1';};
$('ai-config-save').onclick=()=>aiPost('/api/ai-config',`language=${$('ai-language').value}\nauto=${$('ai-auto').checked?1:0}`,'AI settings saved.');

// ---- Events per hour, last 24 h (from the most recent events the server reports).
function renderHistogram(events){
 const now=Date.now()/1000,hour=Math.floor(now/3600),counts=new Array(24).fill(0),alerts=new Array(24).fill(0);
 for(const e of events){const i=23-(hour-Math.floor(e.start/3600));if(i>=0&&i<24){counts[i]++;if(e.ai?.alerts?.length)alerts[i]++;}}
 const max=Math.max(1,...counts);
 $('histogram').replaceChildren(...counts.map((n,i)=>{
  const h=new Date((hour-23+i)*3600*1000).getHours(),bar=text('div','','bar');
  bar.title=`${String(h).padStart(2,'0')}:00 · ${n} event${n===1?'':'s'}${alerts[i]?` · ${alerts[i]} with alerts`:''}`;
  const fill=text('span','');fill.style.height=`${n?Math.max(8,n/max*100):0}%`;if(alerts[i])fill.classList.add('alerted');
  bar.append(fill,text('small',i%6===0||i===23?String(h).padStart(2,'0'):''));return bar;}));
 message('histogram-total',`${counts.reduce((a,b)=>a+b,0)} events`);
}

// ---- Watches that learn: the operator labels alerts; labeled keyframes become reference examples.
const clock=unix=>new Date(unix*1000).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'});
async function label(e,watch,verdict){
 const key=operatorToken();if(!key){needOperator('status');return;}
 try{await request(`/api/feedback/${e.id}`,{key,method:'POST',body:`watch=${watch}\nverdict=${verdict}`});
  message('status',verdict==='clear'?`Label removed from event #${e.id}.`:`Event #${e.id} labeled. The next analyses for this watch see it as a reference example.`);refresh();}
 catch(err){message('status',err.message);}
}
function alertRow(e,a,byId){
 const row=text('div','','event-alert');const verdict=feedback.get(`${e.id}:${a.watch}`);
 row.classList.toggle('dismissed',verdict===false);
 row.append(text('span',`⚠ ${byId.get(a.watch)?.prompt||a.watch} — ${a.reason} (${a.confidence})`));
 const tools=text('span','','label-tools');
 if(verdict===undefined){
  const right=text('button','✓ Right');right.title='Confirm: use this frame as a positive example';right.onclick=()=>label(e,a.watch,'right');
  const wrong=text('button','✗ False alarm');wrong.title='Reject: the model will learn this is not a match';wrong.onclick=()=>label(e,a.watch,'wrong');
  if(byId.has(a.watch))tools.append(right,wrong);
 }else{
  const undo=text('button','Undo');undo.onclick=()=>label(e,a.watch,'clear');
  tools.append(text('em',verdict?'Confirmed · reference example':'False alarm · the model learns from it'),undo);
 }
 row.append(tools);return row;
}
function teachControl(e,byId){
 if(e.end===null)return null;
 const alerted=new Set((e.ai?.alerts||[]).map(a=>a.watch));
 const options=[...byId.values()].filter(w=>w.device===e.device&&!alerted.has(w.id));
 if(!options.length)return null;
 const taught=options.filter(w=>feedback.get(`${e.id}:${w.id}`)===true);
 if(taught.length){const b=text('button',`✓ Example for “${taught[0].prompt.slice(0,40)}” · undo`);b.onclick=()=>label(e,taught[0].id,'clear');return b;}
 const wrap=text('span','','teach');const select=document.createElement('select');select.setAttribute('aria-label','Watch that should have alerted');
 select.append(...options.map(w=>{const o=text('option',w.prompt.slice(0,48));o.value=w.id;return o;}));
 const b=text('button','Should have alerted');b.title='Teach the watch with this event';b.onclick=()=>label(e,select.value,'right');
 wrap.append(select,b);return wrap;
}

// ---- Ignore zones: 8x6 grid; the camera skips these cells when it measures motion.
const zoneMask=id=>{const v=activity?.zones?.[id];return /^[0-9a-f]{12}$/.test(v||'')?BigInt('0x'+v):0n;};
function paintZones(el,mask){[...el.children].forEach((c,i)=>c.classList.toggle('ignored',!!((mask>>BigInt(i))&1n)));el.hidden=!mask;}
let zoneEdit=null;
function closeZones(){if(!zoneEdit)return;if(zoneEdit.url)URL.revokeObjectURL(zoneEdit.url);zoneEdit=null;$('zones').hidden=true;$('zones-img').removeAttribute('src');$('zones-boxes').replaceChildren();}
function drawZoneGrid(){
 const z=zoneEdit;[...$('zones-grid').children].forEach((c,i)=>{c.classList.toggle('ignored',!!((z.mask>>BigInt(i))&1n));c.classList.toggle('proposed',!!((z.proposed>>BigInt(i))&1n));});
 const n=[...z.mask.toString(2)].filter(b=>b==='1').length;message('zones-message',`${n} of 48 cells ignored${z.proposed?' · dashed cells were proposed by SAM 3.1':''}. Save to send the grid to the camera.`);
}
async function openZones(id){
 closeZones();const me={id,mask:zoneMask(id),proposed:0n,url:'',painting:null};zoneEdit=me;
 $('zones').hidden=false;message('zones-title',`Ignore zones · ${id}`);
 $('zones-grid').replaceChildren(...Array.from({length:48},(_,i)=>{const c=text('button','');c.setAttribute('aria-label',`Cell ${i%8+1},${Math.floor(i/8)+1}`);
  const apply=()=>{if(me.painting===null)return;const bit=1n<<BigInt(i);me.mask=me.painting?me.mask|bit:me.mask&~bit;drawZoneGrid();};
  c.onpointerdown=ev=>{ev.preventDefault();me.painting=!((me.mask>>BigInt(i))&1n);apply();};c.onpointerenter=apply;
  c.onkeydown=ev=>{if(ev.key===' '||ev.key==='Enter'){ev.preventDefault();me.painting=!((me.mask>>BigInt(i))&1n);apply();me.painting=null;}};return c;}));
 drawZoneGrid();
 try{const r=await request(`/api/latest/${id}`,{allowMissing:true});if(zoneEdit!==me||!r)return;me.url=URL.createObjectURL(await r.blob());$('zones-img').src=me.url;}catch{}
}
document.addEventListener('pointerup',()=>{if(zoneEdit)zoneEdit.painting=null;});
$('zones-close').onclick=closeZones;$('zones').onclick=e=>{if(e.target===$('zones'))closeZones();};
$('zones-clear').onclick=()=>{if(!zoneEdit)return;zoneEdit.mask=0n;zoneEdit.proposed=0n;$('zones-boxes').replaceChildren();drawZoneGrid();};
$('zones-save').onclick=async()=>{
 const z=zoneEdit;if(!z)return;const key=operatorToken();if(!key){needOperator('zones-message');return;}
 try{await request(`/api/zones/${z.id}`,{key,method:'POST',body:`mask=${z.mask.toString(16).padStart(12,'0')}`});if(zoneEdit!==z)return;
  message('zones-message','Saved. The camera applies it with its next upload.');refresh();}
 catch(e){message('zones-message',e.message.includes('400')?'Keep at least a quarter of the scene watched.':e.message);}
};
$('zones-concept').onkeydown=e=>{if(e.key==='Enter')$('zones-detect').click();};
$('zones-detect').onclick=async()=>{
 const z=zoneEdit,concept=$('zones-concept').value.trim();if(!z||concept.length<2)return;
 const key=operatorToken();if(!key){needOperator('zones-message');return;}
 $('zones-detect').disabled=true;message('zones-message',`SAM 3.1 is looking for “${concept}”…`);
 try{
  const r=await(await request(`/api/zones-suggest/${z.id}`,{key,method:'POST',body:concept,timeoutMs:35000})).json();if(zoneEdit!==z)return;
  if(r.source!=='meta'){message('zones-message','SAM 3.1 needs your Meta key (Watches & rules → Meta AI). Until then, paint the cells by hand.');return;}
  const mask=/^[0-9a-f]{12}$/.test(r.mask)?BigInt('0x'+r.mask):0n;z.proposed=mask;z.mask|=mask;
  $('zones-boxes').replaceChildren(...(r.boxes||[]).slice(0,16).map(b=>{const e=text('span','');Object.assign(e.style,{left:`${b[0]*100}%`,top:`${b[1]*100}%`,width:`${(b[2]-b[0])*100}%`,height:`${(b[3]-b[1])*100}%`});return e;}));
  drawZoneGrid();if(!r.boxes?.length)message('zones-message',`SAM 3.1 found no “${concept}” in the latest frame.`);
 }catch(e){if(zoneEdit===z)message('zones-message',e.message);}
 finally{$('zones-detect').disabled=false;}
};

// ---- Routines and the daily summary.
const minutes=v=>{const m=/^(\d\d):(\d\d)$/.exec(v);return m?Number(m[1])*60+Number(m[2]):NaN;};
const hhmm=m=>`${String(Math.floor(m/60)).padStart(2,'0')}:${String(m%60).padStart(2,'0')}`;
function renderCare(act){
 const routines=act.routines||[];
 $('routine-status').replaceChildren(...(routines.length?routines.map(r=>{
  const row=text('div','',`routine ${r.status}`);const icon=r.status==='seen'?'✓':r.status==='missed'?'⚠':'…';
  row.append(text('strong',`${icon} ${r.label}`),text('span',r.status==='seen'?`first motion ${clock(r.at)} · ${r.device}`:r.status==='missed'?`no motion ${hhmm(r.from)}–${hhmm(r.to)} · ${r.device}`:`expected ${hhmm(r.from)}–${hhmm(r.to)} · ${r.device}`,'muted'));return row;})
  :[text('p','No routines yet. Add one under Watches & rules, for example “Mom gets up” between 07:00 and 10:00.','empty')]));
 $('routine-list').replaceChildren(...(routines.length?routines.map(r=>{
  const row=text('article','','list-row');const c=text('div','');c.append(text('strong',r.label),text('p',`${r.id} · ${r.device} · ${hhmm(r.from)}–${hhmm(r.to)} · today: ${r.status}`));
  const del=text('button','Delete');del.onclick=()=>routinePost(r.id,'delete=1',`Deleted ${r.id}.`);row.append(c,del);return row;})
  :[text('p','No routines yet.','empty')]));
 const d=act.digest;if(d&&!digestBusy)renderDigest(d);
}
async function routinePost(id,body,done){
 const key=operatorToken();if(!key){needOperator('routine-message');return;}
 try{await request(`/api/routines/${id}`,{key,method:'POST',body});message('routine-message',done);refresh();}
 catch(e){message('routine-message',e.message.includes('422')?'Routine limit reached (8). Delete one first.':e.message.includes('400')?'Use a 3–80 character name and an end time after the start time.':e.message);}
}
$('routine-save').onclick=()=>{
 const label=$('routine-label').value.trim(),from=minutes($('routine-from').value),to=minutes($('routine-to').value);
 if(label.length<3||/[\u0000-\u001f]/.test(label)||!(from<to)){message('routine-message','Use a 3–80 character name and an end time after the start time (same day).');return;}
 const ids=new Set((activity?.routines||[]).map(r=>r.id));let n=1;while(ids.has(`routine-${n}`))n++;
 routinePost(`routine-${n}`,`device=${$('routine-device').value}\nlabel=${label}\nfrom=${from}\nto=${to}\noffset=${-new Date().getTimezoneOffset()}\nenabled=1`,`Saved. X-Lens expects motion on ${$('routine-device').value} between ${hhmm(from)} and ${hhmm(to)} every day.`);
 $('routine-label').value='';
};
let digestBusy=false;
function renderDigest(d){
 const box=$('digest');const head=text('div','','ask-head');
 const today=d.day===new Date().toLocaleDateString('en-CA');
 head.append(text('span',sourceLabel(d.source),`source ${d.source}`),text('span',today?`Today · written ${clock(d.at)}`:`From ${d.day}`,'muted'));
 const cites=text('div','','button-row');
 for(const h of d.highlights||[]){const e=activity?.events.find(x=>x.id===h.event);const b=text('button',`▶ #${h.event} · ${h.text}`,'cite');b.disabled=!e;if(e)b.onclick=()=>openPlayer(e);cites.append(b);}
 box.replaceChildren(head,text('h3',d.title),text('p',d.summary,'answer'),cites);
}
$('digest-make').onclick=async()=>{
 const key=operatorToken();if(!key){needOperator('status');return;}const g=generation;
 digestBusy=true;$('digest-make').disabled=true;$('digest').replaceChildren(text('p','Reading today’s events, keyframes and routines…','thinking'));
 try{const d=await(await request('/api/digest',{key,method:'POST',body:'today',timeoutMs:35000,headers:{'X-Utc-Offset':String(-new Date().getTimezoneOffset())}})).json();
  if(g!==generation)return;digestBusy=false;if(activity)activity.digest=d;renderDigest(d);if($('speak-alerts').checked)say(`${d.title}. ${d.summary}`);}
 catch(e){if(g===generation)$('digest').replaceChildren(text('p',e.message));}
 finally{digestBusy=false;$('digest-make').disabled=false;}
};

// ---- Voice: push-to-talk questions. With a Meta key, 16 kHz WAV goes to Muse Voice Transcribe
// through your host; without one, the browser's own recognizer is used. Answers are spoken locally.
let voice=null,askSpoken=false,browserVoiceOk=false;
function say(words){
 if(!window.speechSynthesis||!words)return;speechSynthesis.cancel();
 const u=new SpeechSynthesisUtterance(words.slice(0,600));u.lang=/[¿¡ñáéíóú]|\b(que|hay|una|el|la|los|las|de|en)\b/i.test(words)?'es-ES':'en-US';speechSynthesis.speak(u);
}
function stopVoice(){const v=voice;voice=null;if(!v)return;clearTimeout(v.timer);v.abort?.();v.stream?.getTracks().forEach(t=>t.stop());v.ctx?.close().catch(()=>{});$('ask-mic').classList.remove('recording');$('ask-mic').textContent='🎤';}
function wav(chunks,rate){
 const n=chunks.reduce((a,c)=>a+c.length,0),pcm=new Float32Array(n);let at=0;for(const c of chunks){pcm.set(c,at);at+=c.length;}
 // Resample to 16 kHz when the browser ignored the requested context rate.
 const out=rate===16000?pcm:Float32Array.from({length:Math.floor(n*16000/rate)},(_,i)=>pcm[Math.min(n-1,Math.floor(i*rate/16000))]);
 const buf=new DataView(new ArrayBuffer(44+out.length*2));const str=(o,s)=>[...s].forEach((c,i)=>buf.setUint8(o+i,c.charCodeAt(0)));
 str(0,'RIFF');buf.setUint32(4,36+out.length*2,true);str(8,'WAVE');str(12,'fmt ');buf.setUint32(16,16,true);buf.setUint16(20,1,true);buf.setUint16(22,1,true);
 buf.setUint32(24,16000,true);buf.setUint32(28,32000,true);buf.setUint16(32,2,true);buf.setUint16(34,16,true);str(36,'data');buf.setUint32(40,out.length*2,true);
 out.forEach((v,i)=>buf.setInt16(44+i*2,Math.max(-1,Math.min(1,v))*0x7fff,true));return new Blob([buf],{type:'audio/wav'});
}
async function transcribeMeta(key){
 const stream=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true},video:false});
 const ctx=new AudioContext({sampleRate:16000}),src=ctx.createMediaStreamSource(stream),proc=ctx.createScriptProcessor(4096,1,1),chunks=[];
 const me={stream,ctx,chunks};voice=me;proc.onaudioprocess=e=>{if(voice===me)chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));};src.connect(proc);proc.connect(ctx.destination);
 $('ask-mic').classList.add('recording');$('ask-mic').textContent='■';message('status','Listening… press ■ to send (max 12 s). Audio goes to your host, then Meta Muse Voice Transcribe.');
 await new Promise(resolve=>{me.finish=resolve;me.timer=setTimeout(resolve,12000);});
 const rate=ctx.sampleRate;if(voice===me)stopVoice();
 if(!chunks.length)throw Error('No audio captured.');
 message('status','Transcribing with Muse Voice Transcribe…');
 const r=await(await request('/api/transcribe',{key,method:'POST',body:wav(chunks,rate),timeoutMs:35000,headers:{'Content-Type':'audio/wav','X-Language':($('ai-language').value||'en')}})).json();
 if(typeof r.text!=='string')throw Error('Invalid transcription.');return r.text;
}
function transcribeBrowser(){
 const Recognition=window.SpeechRecognition||window.webkitSpeechRecognition;
 if(!Recognition)throw Error('Voice needs your Meta key (Muse Voice Transcribe) or a browser with speech recognition.');
 if(!browserVoiceOk){if(!confirm('Without a Meta key, voice uses your browser’s speech recognition, which may send audio to the browser vendor. Continue?'))throw Error('Voice cancelled. Typing works offline.');browserVoiceOk=true;}
 return new Promise((resolve,reject)=>{
  const r=new Recognition();const me={abort:()=>r.abort()};voice=me;r.lang=({es:'es-ES',pt:'pt-BR',fr:'fr-FR',de:'de-DE',it:'it-IT'})[$('ai-language').value]||'en-US';r.interimResults=false;r.continuous=false;
  me.finish=()=>r.stop();me.timer=setTimeout(()=>r.stop(),12000);
  r.onresult=e=>resolve(Array.from(e.results).map(v=>v[0].transcript).join(' '));r.onerror=e=>reject(Error(`Recognition failed (${e.error}).`));r.onend=()=>{if(voice===me)stopVoice();resolve('');};
  $('ask-mic').classList.add('recording');$('ask-mic').textContent='■';message('status','Listening with the browser recognizer…');r.start();
 });
}
$('ask-mic').onclick=async()=>{
 if(voice){voice.finish?.();return;}
 const key=operatorToken();if(!key){$('ask-answer').hidden=false;$('ask-answer').replaceChildren(text('p','Voice questions use AI budget, so they need the operator token.'));return;}
 const g=generation;
 try{
  const words=(activity?.ai?.mode==='meta'?await transcribeMeta(key):await transcribeBrowser()).trim().slice(0,300);
  if(g!==generation)return;if(!words){message('status','Nothing heard. Try again closer to the microphone.');return;}
  $('ask-input').value=words;askSpoken=true;message('status',`Heard: “${words}”`);$('ask-send').click();
 }catch(e){if(g===generation)message('status',e.name==='NotAllowedError'?'Microphone permission denied. Typing still works.':e.message);}
 finally{stopVoice();}
};
$('routine-edit').onclick=()=>showTab('rules');
