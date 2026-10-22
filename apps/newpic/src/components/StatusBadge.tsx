export function StatusBadge({ok,children}:{ok:boolean;children:string}){return <span className={ok?"badge ok":"badge bad"}>{ok?"✓":"!"} {children}</span>}
