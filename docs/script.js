(function(){
  // Copy buttons on code blocks.
  document.querySelectorAll('pre:not(.term)').forEach(function(pre){
    var b=document.createElement('button');b.className='copy';b.textContent='Copy';
    b.onclick=function(){
      navigator.clipboard.writeText(pre.innerText.replace(/^Copy\n?/,'')).then(function(){
        b.textContent='Copied';setTimeout(function(){b.textContent='Copy'},1200);
      });
    };
    pre.appendChild(b);
  });

  // "On this page" for pages with several sections.
  var main=document.querySelector('main'),hs=main.querySelectorAll('h2[id]');
  if(hs.length>=4){
    var t=document.createElement('div');t.className='toc';
    t.innerHTML='<b>On this page</b><ul>'+Array.prototype.map.call(hs,function(h){
      return '<li><a href="#'+h.id+'">'+h.textContent+'</a></li>'}).join('')+'</ul>';
    var lead=main.querySelector('p.lead');
    (lead||main.querySelector('h1')).insertAdjacentElement('afterend',t);
  }

  // Search.
  var q=document.getElementById('q'),out=document.getElementById('results'),idx=null,sel=-1;
  function load(){
    if(idx)return Promise.resolve(idx);
    return fetch('search-index.json').then(function(r){return r.json()}).then(function(j){idx=j;return j}).catch(function(){idx=[];return idx});
  }
  function esc(s){return s.replace(/[&<>]/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;'}[c]})}
  function run(){
    var terms=q.value.toLowerCase().split(/\s+/).filter(Boolean);
    if(!terms.length){out.innerHTML='';return}
    load().then(function(d){
      var hits=[];
      d.forEach(function(p){
        var tl=p.t.toLowerCase(),xl=p.x.toLowerCase(),score=0,ok=true;
        terms.forEach(function(w){
          var inT=tl.indexOf(w)>=0,inX=xl.indexOf(w)>=0;
          if(!inT&&!inX)ok=false;score+=(inT?10:0)+(inX?1:0);
        });
        if(ok){
          var i=xl.indexOf(terms[0]),from=Math.max(0,i-40);
          hits.push({p:p,score:score,snip:i<0?p.x.slice(0,90):(from?'…':'')+p.x.slice(from,from+110)});
        }
      });
      hits.sort(function(a,b){return b.score-a.score});
      sel=-1;
      out.innerHTML=hits.length?hits.slice(0,8).map(function(h){
        return '<a href="'+h.p.u+'"><b>'+esc(h.p.t)+'</b><span>'+esc(h.snip)+'</span></a>'}).join(''):'<a href="#" onclick="return false"><span>No results</span></a>';
    });
  }
  q.addEventListener('input',run);
  q.addEventListener('keydown',function(e){
    var as=out.querySelectorAll('a[href]');
    if(e.key==='ArrowDown'||e.key==='ArrowUp'){
      e.preventDefault();if(!as.length)return;
      sel=(sel+(e.key==='ArrowDown'?1:-1)+as.length)%as.length;
      as.forEach(function(a,i){a.classList.toggle('sel',i===sel)});
    }else if(e.key==='Enter'&&as.length){as[Math.max(sel,0)].click()}
    else if(e.key==='Escape'){q.value='';out.innerHTML='';q.blur()}
  });
  document.addEventListener('keydown',function(e){
    if(e.key==='/'&&document.activeElement!==q&&!/input|textarea/i.test(document.activeElement.tagName)){e.preventDefault();q.focus()}
  });
  document.addEventListener('click',function(e){if(!e.target.closest('.search'))out.innerHTML=''});
})();
