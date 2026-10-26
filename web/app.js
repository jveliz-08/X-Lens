'use strict';
const $ = id => document.getElementById(id);
let token = '', pollTimer, generation = 0, urls = [], devices = [], savedRules = [], draft = null;
let refreshEpoch = 0, proposalEpoch = 0, proposalController;
let tab = 'overview', stream = null, cameraEpoch = 0, captureTimer, uploadController, uploading = false;
let speech = null, speechEpoch = 0, speechTimer;
const inflight = new Set();
const text = (tag, value, className) => { const e = document.createElement(tag); e.textContent = value; if(className) e.className=className; return e; };
const message = (id, value) => { $(id).textContent=value; };
function revoke(){ urls.forEach(URL.revokeObjectURL); urls=[]; }
async function request(path,{key=token,method='GET',body,allowMissing=false,signal,timeoutMs=6000}={}) {
 const controller=new AbortController();inflight.add(controller);
 const timeout=setTimeout(()=>controller.abort(),timeoutMs);
 const abort=()=>controller.abort();signal?.addEventListener('abort',abort,{once:true});
 if(signal?.aborted) controller.abort();
 try {
  const headers={Authorization:`Bearer ${key}`,'X-Protocol':'1'};
  if(body) headers['Content-Type']=body instanceof Blob?'image/jpeg':'text/plain';
  const r=await fetch(path,{method,body,headers,cache:'no-store',signal:controller.signal});
  if(allowMissing && r.status===404) return null;
  if(!r.ok) throw Error(({401:'Access denied. Check the credential for this action.',409:'Sequence conflict. Stop other sources using this device identity, then reconnect.',422:'Rule capacity reached. Edit an existing rule instead.',426:'Protocol version mismatch. Update this client.',429:'Upload budget reached. Wait, then restart snapshots.',503:'Storage unavailable. Operator recovery is required.'})[r.status] || `Request failed (${r.status}).`);
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
 token='';devices=[];savedRules=[];revoke();
 for(const id of ['token','source-key','operator-key','proposal-key','rule-intent','rule-json']) $(id).value='';
 for(const id of ['cameras','incidents','events','rules-list']) $(id).replaceChildren();
 $('speech-consent').checked=false;$('proposal-consent').checked=false;$('login').hidden=false;$('dashboard').hidden=true;$('lock').hidden=true;
 $('connect').disabled=false;message('connection','LOCKED');message('status','Session locked. Camera and microphone stopped.');
}
function showTab(next) {
 if(tab==='sources' && next!==tab) stopCamera('Camera stopped when leaving Sources.');
 if(tab==='rules' && next!==tab){stopSpeech();cancelProposal();}
 tab=next;document.querySelectorAll('.view').forEach(e=>e.hidden=e.id!==`view-${tab}`);
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
 clearTimeout(pollTimer);if(!token)return;const g=generation,run=++refreshEpoch;const pending=[];
 $('refresh').disabled=true;
 try {
  const [list,signals,rules,events,provider]=await Promise.all(['/api/devices','/api/security','/api/rules','/api/events','/api/rule-provider'].map(async p=>(await request(p)).json()));
  const cards=[],incidents=[];
  for(const d of list) {
   const card=text('article','', 'camera');
   if(d.sequence) {const r=await request(`/api/latest/${d.id}`);const url=URL.createObjectURL(await r.blob());pending.push(url);const img=document.createElement('img');img.src=url;img.alt=`Latest snapshot from ${d.id}`;card.append(img);}
   else card.append(text('div','Waiting for the first snapshot','empty'));
   card.append(text('h3',d.id),text('p',`Sequence ${d.sequence} · ${d.retained} retained · Snapshot, not a live stream`));cards.push(card);
   const response=await request(`/api/review/${d.id}`,{allowMissing:true});
   if(response) {
    const report=await response.json();const item=text('article','', 'incident');
    item.append(text('strong',report.source==='meta'?'META AI · ADVISORY':'SIMULATED AI · NO PROVIDER CALL','ai-source'),text('h3',d.id),text('p',report.assessment?.incident_summary || 'Invalid report'),text('p',`Local policy: ${report.policy?.deterministic_recommendation || 'unknown'} · Human approval required · No actions executed`),text('p',`Evidence sequence ${report.evidence?.sequence ?? '?'} · SHA-256 ${(report.evidence?.image_sha256||'').slice(0,12)}`,'muted'),text('p',`Reviewed ${new Date(report.created_unix*1000).toLocaleString()}. May predate the latest snapshot.`,'muted'));incidents.push(item);
   }
  }
  if(g!==generation||run!==refreshEpoch){pending.forEach(URL.revokeObjectURL);return;}
  message('rule-provider',provider.source==='meta'?'META · LIVE CLOUD (explicit request only)':provider.source==='mock'?'SIMULATED · NO PROVIDER CALL':'AI DISABLED · LOCAL TEMPLATE available');$('rule-ai').disabled=provider.source==='disabled';devices=list;savedRules=rules;populate($('source-device'),list.map(d=>d.id));populate($('rule-device'),list.map(d=>d.id));
  revoke();urls=pending;$('cameras').replaceChildren(...cards);$('incidents').replaceChildren(...(incidents.length?incidents:[text('p','No incident review yet. Run the synthetic incident demo or an explicitly authorized Meta review. No automatic cloud call runs here.','empty')]));
  $('rules-list').replaceChildren(...renderRules(rules));
  $('events').replaceChildren(...(events.length?events.map(e=>{const row=text('article','', 'list-row');row.append(text('strong',e.rule),text('p',`${e.device} · ${e.signal}: ${e.value} · ${new Date(e.at*1000).toLocaleTimeString()} · Panel event only`));return row;}):[text('p','No local rule events. Rules evaluate once per second on the host, even with the dashboard closed.','empty')]));
  message('count',String(list.length));message('auth-count',String(signals.auth_failures));message('replay-count',String(signals.devices.reduce((n,d)=>n+d.replays,0)));message('rule-count',String(rules.filter(r=>r.enabled).length));
  message('security-signals','Counters cover this server session. A replay or stale camera is a review signal, not proof of intrusion.');
  $('login').hidden=true;$('dashboard').hidden=false;$('lock').hidden=false;message('connection','CONNECTED');message('status',`Updated ${new Date().toLocaleTimeString()} · Refresh every 5 seconds. Capture/storage/rules stay local; Meta analysis is separate and opt-in.`);
 } catch(e) {pending.forEach(URL.revokeObjectURL);if(g!==generation||run!==refreshEpoch)return;message('connection','STALE / UNAVAILABLE');message('status',`${e.name==='AbortError'?'Request interrupted or timed out.':e.message} Previously displayed data may be stale.`);}
 finally {if(g===generation&&run===refreshEpoch){$('connect').disabled=false;$('refresh').disabled=false;if(token)pollTimer=setTimeout(refresh,5000);}}
}
$('connect').onclick=()=>{generation++;clearTimeout(pollTimer);token=$('token').value.trim();$('token').value='';if(!/^[a-fA-F0-9]{64}$/.test(token)){message('status','Enter a 64-character hexadecimal viewer token.');token='';return;}$('connect').disabled=true;refresh();};
$('token').onkeydown=e=>{if(e.key==='Enter'&&!$('connect').disabled)$('connect').click();};
$('lock').onclick=lock;$('refresh').onclick=refresh;
for(const e of document.querySelectorAll('[data-tab]'))e.onclick=()=>showTab(e.dataset.tab);
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
 uploading=true;$('upload-start').disabled=true;$('source-device').disabled=true;uploadController=new AbortController();const signal=uploadController.signal;
 try{
  const state=await(await request(`/api/device-state/${id}`,{key,signal})).json();let next=BigInt(state.next_sequence);
  async function capture(){
   if(epoch!==cameraEpoch||session!==generation||!stream||!uploading)return;
   const canvas=document.createElement('canvas');canvas.width=320;canvas.height=240;
   try{
    if($('preview').readyState<2)throw Error('No camera frame available.');
    canvas.getContext('2d').drawImage($('preview'),0,0,320,240);
    const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/jpeg',.75));
    if(epoch!==cameraEpoch||session!==generation||!stream)return;
    if(!blob||blob.size>131072)throw Error('JPEG exceeds the 128 KiB limit. Capture stopped.');
    const seq=next++;const controller=new AbortController();uploadController=controller;const timeout=setTimeout(()=>controller.abort(),6000);
    try{const r=await fetch(`/api/frame/${id}`,{method:'POST',headers:{Authorization:`Bearer ${key}`,'Content-Type':'image/jpeg','X-Sequence':seq.toString(),'X-Protocol':'1'},body:blob,signal:controller.signal});
     if(!r.ok)throw Error(r.status===409?'Sequence conflict: another source may share this identity.':r.status===429?'Rate limited. Wait before reconnecting.':`Upload rejected (${r.status}).`);
    }finally{clearTimeout(timeout);}
    if(epoch!==cameraEpoch||session!==generation)return;
    message('camera-status',`Uploading · sequence ${seq} · ${blob.size} bytes · next snapshot in 5 s`);captureTimer=setTimeout(capture,5000);
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
document.addEventListener('visibilitychange',()=>{if(document.hidden){stopCamera('Camera stopped while the page was hidden.');stopSpeech();}});

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
