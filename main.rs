use std::{io::{Read,Write},net::TcpListener};
use serde_json::{json,Value};use solana_escrow_lab::Ledger;
fn handle(path:&str,data:Value,l:&mut Ledger)->Result<Value,String>{
 match path{
  "/api/create"=>{l.create(data["amount"].as_u64().ok_or("Integer amount required")?,data["duration"].as_i64().ok_or("Duration required")?)?;},
  "/api/release"|"/api/refund"=>{l.settle(data["id"].as_u64().ok_or("Deal ID required")?,data["actor"].as_str()==Some("buyer"),path=="/api/refund")?;},
  "/api/advance"=>{l.advance(data["seconds"].as_i64().ok_or("Time step required")?)?;},_=>return Err("Unknown action".into())
 };Ok(serde_json::to_value(l).unwrap())
}
fn main()->std::io::Result<()>{
 let port=std::env::var("PORT").unwrap_or("8765".into());let addr=format!("127.0.0.1:{port}");let listener=TcpListener::bind(&addr)?;let mut ledger=Ledger::default();println!("Local Rust simulator: http://{addr}");
 for stream in listener.incoming(){let mut stream=stream?;stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
  let mut request=Vec::new();let mut buf=[0u8;4096];let mut body_start=0;let mut size=0;let mut valid=true;
  loop{match stream.read(&mut buf){Ok(0)|Err(_)=>{valid=false;break},Ok(n)=>{request.extend(&buf[..n]);if request.len()>65536{valid=false;break}}}
   if let Some(pos)=request.windows(4).position(|w|w==b"\r\n\r\n"){body_start=pos+4;let header=String::from_utf8_lossy(&request[..pos]);size=header.lines().find_map(|line|line.to_ascii_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap_or(65537))).unwrap_or(0);if size>60000{valid=false;break}if request.len()>=body_start+size{break}}
  }
  if !valid{continue}let head=String::from_utf8_lossy(&request[..body_start]);let mut line=head.lines().next().unwrap_or("").split_whitespace();let method=line.next().unwrap_or("");let path=line.next().unwrap_or("");let host=head.lines().find_map(|s|s.strip_prefix("Host: ").or_else(||s.strip_prefix("host: "))).unwrap_or("");let origin=head.lines().find_map(|s|s.strip_prefix("Origin: ").or_else(||s.strip_prefix("origin: ")));
  let (status,mime,body)=if host!=addr&&host!=format!("localhost:{port}"){(403,"application/json",json!({"error":"Invalid host"}).to_string().into_bytes())}
  else if method=="GET"&&path=="/"{(200,"text/html; charset=utf-8",std::fs::read("index.html")?)}
  else if method=="GET"&&path=="/api/state"{(200,"application/json",serde_json::to_vec(&ledger)?)}
  else if method=="POST"&&origin.map(|o|o!=format!("http://{host}")).unwrap_or(false){(403,"application/json",json!({"error":"Invalid origin"}).to_string().into_bytes())}
  else if method=="POST"{match serde_json::from_slice(&request[body_start..body_start+size]).map_err(|_|"Invalid JSON".to_string()).and_then(|v|handle(path,v,&mut ledger)){Ok(value)=>(200,"application/json",value.to_string().into_bytes()),Err(error)=>(400,"application/json",json!({"error":error}).to_string().into_bytes())}}
  else{(404,"application/json",b"{\"error\":\"Not found\"}".to_vec())};
  let header=format!("HTTP/1.1 {status} Result\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",body.len());let _=stream.write_all(header.as_bytes());let _=stream.write_all(&body);
 }Ok(())
}
