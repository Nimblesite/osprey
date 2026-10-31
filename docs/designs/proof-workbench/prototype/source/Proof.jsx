import {useEffect,useRef} from 'react';
import {ArrowRight, CaretRight, X, Code, MagnifyingGlass} from '@phosphor-icons/react';
import {steps, goalAt, symbols, sourceLines, rememberSelection, insertSymbol} from './model';
import {MathText, Sentence} from './Math';
import {SourceDownload} from './Toolbar';
import {Solution,UnverifiedWorking} from './Solution';
const set = (dispatch,value) => dispatch({value});
export function Goal({state}) {
  if(state.freeform)return <UnverifiedWorking/>;
  const count = Math.min(state.selected,state.count);
  return <aside className="goal" aria-label="Current goal"><div className="eyebrow">At this step</div><h3>{count === 5 ? 'The draft is complete.' : 'What remains to show'}</h3><div className="hypotheses"><span className="micro">{count === 0 ? 'Propositions' : 'Hypotheses'}</span>{count === 0 ? <MathText value="p, q"/> : count === 1 ? <MathText value="h : p ∧ q"/> : <><MathText value="hp : p"/><MathText value="hq : q"/></>}</div><div className="goal-target" key={count}>{count === 5 ? <><span className="micro">To show</span><p>No open draft goals</p></> : <><span className="micro">To show</span><MathText value={goalAt(count)} display/></>}</div><p className="fine">{count === 5 ? 'Still unchecked. No kernel has verified this document.' : `Draft goal after ${count} ${count === 1 ? 'step' : 'steps'}.`}</p>{count === 3 && <div className="next-goal"><span className="micro">Then</span><MathText value="p"/></div>}</aside>;
}
export function Proof({state,dispatch}) {
  if(state.freeform)return <Solution state={state} dispatch={dispatch}/>;
  return <article className="proof"><div className="eyebrow">A first argument · propositional logic</div><h1>Commutativity<br className="board-break"/> of conjunction</h1><div className="verdict"><span className="badge">Unchecked</span><span>No checker on this host</span></div><p className="statement">For all propositions <MathText value="p"/> and <MathText value="q"/>.</p><div className="claim"><MathText value="p ∧ q → q ∧ p" display/></div><h2>Proof.</h2><ol className="steps">{steps.slice(0,state.count).map((text,index) => <li key={text} className={state.selected === index + 1 ? 'selected' : ''}><button onClick={() => set(dispatch,{selected:index+1})}><span className="step-number">{index+1}</span><span><Sentence text={text}/></span></button></li>)}</ol>{state.mode !== 'read' && <StepEditor state={state} dispatch={dispatch}/ >}{state.count === 5 && <div className="draft-complete"><span>Draft complete · remains unchecked</span></div>}</article>;
}
function StepEditor({state,dispatch}) {
  const mismatch=Boolean(state.error)&&state.count===3&&state.input.trim()==='From hp.';
  if (state.count === 5) return <button className="text-button" onClick={() => dispatch({type:'reset'})}>Start the example again <ArrowRight/></button>;
  return <div className="editor-wrap"><form className={`step-editor ${state.error ? mismatch?'invalid':'unsupported' : ''}`} onSubmit={event => {event.preventDefault(); dispatch({type:'step'});}}><label htmlFor="step-entry" className="step-number">{state.count+1}</label><input id="step-entry" value={state.input} onSelect={event=>rememberSelection(dispatch,event,'input')} placeholder="Write the next step…" aria-label="Next proof step" aria-describedby={state.error?'step-error':'step-help'} aria-invalid={mismatch} onChange={event => set(dispatch,{input:event.target.value,error:'',selected:state.count})}/><button aria-label="Add proof step" type="submit"><ArrowRight size={20}/></button></form>{state.error ? <div id="step-error" className={`mismatch ${mismatch?'':'unsupported'}`} role={mismatch?'alert':'status'}><strong>{mismatch?'The goal and this step differ.':'This sentence is outside this example.'}</strong><p>{state.error}</p>{mismatch && <div className="comparison"><span>Needed <MathText value={goalAt(state.count)}/></span><span>This step gives <MathText value="p"/></span></div>}<button className="text-button" onClick={() => set(dispatch,{input:steps[state.count],error:''})}>Use the example step <ArrowRight size={16}/></button></div> : <div id="step-help" className="editor-help"><span>Try “{steps[state.count]}”</span><button onClick={() => set(dispatch,{input:steps[state.count]})}>Insert example</button></div>}</div>;
}
function Source({state}) {
  if(state.freeform)return <><h3>Optional generated source</h3><p className="fine">Your free-form working is kept as written. This prototype cannot generate checked source for arbitrary mathematics.</p><p className="fine">Play the guided walkthrough to inspect its syntax-highlighted example.</p></>;
  return <><div className="panel-title"><Code/><h3>Behind the mathematics</h3></div><p className="fine">Generated ML syntax · read only · unchecked</p><pre className="source"><code>{sourceLines(state.count).map((line,index) => <span className={`code-line ${index === Math.max(3,state.selected+2) ? "code-active" : ""}`} key={index}><span className="line-no">{index+1}</span><Highlighted text={line}/></span>)}</code></pre><p className="fine">This is a design fixture. In the application, the same file goes to the proof checker. You never need to edit it.</p><SourceDownload state={state}/></>;
}
function Highlighted({text}) {
  if (text.startsWith('//')) return <span className="syntax-comment">{text}</span>;
  return <>{text.split(/(\b(?:namespace|theorem|by|cases|with|constructor|exact|sorry|Prop)\b|and_swap|:=|∧)/g).map((word,index) => <span key={index} className={word === 'sorry' ? 'syntax-hole' : /^(namespace|theorem|by|cases|with|constructor|exact)$/.test(word) ? 'syntax-keyword' : word === 'Prop' ? 'syntax-type' : word === 'and_swap' ? 'syntax-name' : /^(∧|:=)$/.test(word) ? 'syntax-operator' : ''}>{word}</span>)}</>;
}
export function Panel({state,dispatch}) {
  const ref=useRef(null);
  useEffect(()=>{if(!state.panel||ref.current.closest('[inert]'))return;const trigger=document.activeElement;ref.current.querySelector('button')?.focus({preventScroll:true});ref.current.scrollIntoView({block:'nearest'});return()=>trigger?.focus({preventScroll:true});},[state.panel]);
  if (!state.panel) return null;
  return <section ref={ref} onKeyDown={event=>{if(event.key==='Escape'){event.stopPropagation();set(dispatch,{panel:''});}}} className={`utility-panel ${state.panel==='source'?'source-panel':''}`} aria-label={`${state.panel} panel`}><button className="close-panel" aria-label="Close panel" onClick={() => set(dispatch,{panel:''})}><X size={20}/></button>{state.panel === 'source' ? <Source state={state}/> : state.panel === 'notation' ? <Notation state={state} dispatch={dispatch}/> : state.panel === 'library' ? <Library dispatch={dispatch}/> : <Recognition dispatch={dispatch}/>}</section>;
}
function Notation({state,dispatch}) {
  return <><div className="panel-title"><h3>The language you already use.</h3></div><p className="fine">{state.freeform ? 'Insert at the cursor in your written working.' : 'Insert at the cursor in the next proof step.'}</p><div className="symbol-grid">{symbols.map(symbol => <button key={symbol} onClick={() => insertSymbol(state,dispatch,symbol)}><MathText value={symbol}/></button>)}</div><p className="fine">In the application, backslash abbreviations and direct Unicode enter the same mathematics.</p></>;
}
function Library({dispatch}) {
  return <><div className="panel-title"><MagnifyingGlass/><h3>A familiar starting point</h3></div><p className="fine">Example library · one local design fixture</p><div className="library-result"><span className="eyebrow">Propositional logic</span><h3>Conjunction, in either order</h3><MathText value="p ∧ q → q ∧ p" display/><p>Work through a small argument, one sentence at a time.</p><button onClick={() => dispatch({type:'reset'})}>Open example <ArrowRight/></button></div><p className="fine">Imported theorem search belongs to the application’s later library phase.</p></>;
}
function Recognition({dispatch}) {
  return <><div className="panel-title"><h3>From a sketch to a statement.</h3></div><p className="fine">Sample transcription preview. Handwriting recognition is not connected.</p><div className="recognised"><MathText value="p ∧ q → q ∧ p" display/></div><p>Review the mathematics before adding it to your document.</p><button className="primary" onClick={() => set(dispatch,{panel:'',freeform:false,sample:true,mode:'paper',count:0,selected:0,input:'',error:'',complete:false})}>Use this sample statement <CaretRight/></button><p className="fine">Your strokes remain on the board. Nothing becomes a proof or a checked result automatically.</p></>;
}
