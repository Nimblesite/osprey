import React from 'react';
export function MathText({value, display=false}) {
  const tokens = value.match(/[a-zA-Z]+|[0-9]+|[^\s]/g) || [];
  return <math display={display ? 'block' : 'inline'} aria-label={value}><mrow>{tokens.map((token,index) => /^[a-zA-Z]+$/.test(token) ? <mi key={index} mathvariant="italic">{token}</mi> : /^[0-9]+$/.test(token) ? <mn key={index}>{token}</mn> : <mo key={index}>{token}</mo>)}</mrow></math>;
}
export function Sentence({text}) {
  return <>{text.split(/(p ∧ q|q ∧ p|\bhp\b|\bhq\b|\bh\b|\bp\b|\bq\b)/g).map((part,index) => /^(p ∧ q|q ∧ p|hp|hq|h|p|q)$/.test(part) ? <MathText key={index} value={part}/> : <React.Fragment key={index}>{part}</React.Fragment>)}</>;
}
