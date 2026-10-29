// Design fixtures only. This prototype never invokes a proof checker.
export const steps = ['Assume h : p ∧ q.', 'By cases on h, as hp and hq.', 'It suffices to show q and p.', 'From hq.', 'From hp.'];
export const symbols = ['∀', '∃', '→', '↔', '∧', '∨', '¬', '⊢', 'ℕ', 'ℤ', 'ℝ', '≤', '≥', '≠', '∈'];
export const initial = {mode:'board', theme:'paper', count:3, selected:3, input:'', error:'', panel:'', complete:false, toast:'', sample:false, freeform:true, title:'Untitled solution', working:'', paths:[], notes:'', view:{zoom:1,x:0,y:0}};
export const goalAt = count => ['p ∧ q → q ∧ p','q ∧ p','q ∧ p','q','p',''][count];
export const chapters = [
  {title:'Room to think.', copy:'Draw, calculate and explore. Your working stays with you.', mode:'board', count:0, panel:''},
  {title:'Bring the idea into focus.', copy:'Review a transcription before it becomes part of your solution.', mode:'board', count:0, panel:'recognition'},
  {title:'Write it as you would explain it.', copy:'Ordinary mathematical sentences, with the goal beside you.', mode:'paper', count:2, panel:''},
  {title:'See exactly where it differs.', copy:'Compare what the goal needs with what your step establishes.', mode:'paper', count:3, panel:'mismatch'},
  {title:'One idea at a time.', copy:'Resolve the first goal; the next takes its place quietly.', mode:'paper', count:4, panel:''},
  {title:'Keep the whole argument.', copy:'A completed draft is distinct from a kernel-checked proof.', mode:'paper', count:5, panel:''},
  {title:'Go deeper, if you choose.', copy:'The generated source follows the mathematics. It is always optional.', mode:'paper', count:5, panel:'source'},
  {title:'From working to a readable page.', copy:'Read and export your solution, with its verification status intact.', mode:'read', count:5, panel:''},
  {title:'Stay at the board.', copy:'A quiet slate appearance, ready for the next idea.', mode:'board', count:5, panel:'', theme:'slate'},
];
export function applyStep(state) {
  const input = state.input.trim();
  if (!input) return state;
  if (input !== steps[state.count]) return {...state, error: input === 'From hp.' && state.count === 3 ? 'This step establishes p. The current goal is q.' : 'This design prototype demonstrates the and_swap example only. This sentence has not been checked.'};
  const count = state.count + 1;
  return {...state, count, selected:count, input:'', error:'', complete:count === steps.length};
}
export function reducer(state, action) {
  if (action.type === 'step') return applyStep(state);
  if (action.type === 'reset') return {...state, count:0, selected:0, input:'', error:'', complete:false, panel:'', freeform:false};
  if (action.type === 'scene') { const scene = chapters[action.index]; return {...state, ...scene, freeform:false, sample:true, theme:scene.theme || 'paper', selected:scene.count, input:scene.panel === 'mismatch' ? 'From hp.' : '', error:scene.panel === 'mismatch' ? 'This step establishes p. The current goal is q.' : '', complete:scene.count === 5, panel:scene.panel === 'mismatch' ? '' : scene.panel}; }
  return {...state, ...action.value};
}
export const sourceLines = count => [
  '// osprey: types=lean4', 'namespace proofs', '',
  count ? 'theorem and_swap (p q : Prop) (h : p ∧ q) : q ∧ p := by' : 'theorem and_swap (p q : Prop) : p ∧ q → q ∧ p := by',
  ...(count >= 2 ? ['    cases h with hp hq'] : []),
  ...(count >= 3 ? ['    constructor'] : []),
  ...(count >= 4 ? ['    exact hq'] : []),
  ...(count >= 5 ? ['    exact hp'] : ['    sorry']),
];

export const rememberSelection=(dispatch,event,field)=>dispatch({value:{selection:{field,start:event.target.selectionStart,end:event.target.selectionEnd}}});
export function insertSymbol(state,dispatch,symbol) {
  const field=state.freeform?'working':'input', text=state[field], selection=state.selection?.field===field?state.selection:{start:text.length,end:text.length};
  const start=Math.min(selection.start,text.length), end=Math.min(selection.end,text.length), caret=start+symbol.length;
  dispatch({value:{[field]:text.slice(0,start)+symbol+text.slice(end),error:'',selection:{field,start:caret,end:caret}}});
  queueMicrotask(()=>{const editor=document.querySelector(field==='working'?'.solution-editor':'#step-entry');editor?.focus();editor?.setSelectionRange(caret,caret);});
}
