import {rememberSelection} from './model';
export function Solution({state,dispatch}) {
 const set=value=>dispatch({value});
 return <article className="proof solution"><label className="eyebrow" htmlFor="solution-title">Your solution</label><h1><input id="solution-title" aria-label="Solution title" value={state.title} onChange={event=>set({title:event.target.value})}/></h1><div className="verdict"><span className="badge">Unchecked</span><span>Free-form working</span></div>{state.mode==='read'?<div className="solution-reading">{state.working || 'Your written working will appear here.'}</div>:<label className="writing-field"><span className="micro">Argument & calculations</span><textarea className="solution-editor" aria-label="Write your solution" value={state.working} onSelect={event=>rememberSelection(dispatch,event,'working')} onChange={event=>set({working:event.target.value})} placeholder={'State the problem, then work through your reasoning…'}/></label>}<p className="fine save-hint">Save keeps the board and your written working.</p></article>;
}
export function UnverifiedWorking() {
 return <aside className="goal freeform-goal"><span className="micro">No checker connected</span><p className="fine">Your working stays unverified. Open Examples to explore guided proof steps.</p></aside>;
}
