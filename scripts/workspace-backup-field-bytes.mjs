// Test-only byte-span inspection AFTER Core validated a definite synthetic fixture.
// This is never a snapshot/import validator and does not interpret field semantics.
export function fixtureFieldBytes(bytes) {
  let position = 0;
  const byte = () => {if(position >= bytes.length)throw new Error("truncated fixture bytes");return bytes[position++];};
  function item(depth = 0) {
    if(depth > 16)throw new Error("fixture nesting exceeded");
    const initial=byte(),major=initial>>5,info=initial&31;
    let value=info;
    if(info>=24&&info<=27){value=0;for(let width=2**(info-24);width>0;width--)value=value*256+byte();}
    else if(info>=28)throw new Error("indefinite fixture item");
    if(major===2||major===3){if(!Number.isSafeInteger(value)||position+value>bytes.length)throw new Error("fixture byte span exceeded");position+=value;}
    else if(major===4||major===5){if(!Number.isSafeInteger(value)||value>bytes.length)throw new Error("fixture container exceeded");for(let count=value*(major===5?2:1);count>0;count--)item(depth+1);}
    else if(major!==0&&major!==1&&!(major===7&&info<=23))throw new Error("unsupported fixture item");
    return {major,value};
  }
  const header=byte();if(header>>5!==5||header%32>=24)throw new Error("unexpected fixture map header");
  const result=new Map();
  for(let count=header%32;count>0;count--){const key=item();if(key.major!==0||!Number.isSafeInteger(key.value)||result.has(key.value))throw new Error("invalid fixture key");const start=position;item();result.set(key.value,bytes.subarray(start,position));}
  if(position!==bytes.length)throw new Error("trailing fixture bytes");return result;
}
