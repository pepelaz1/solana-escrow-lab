use serde::{Serialize, Deserialize};
#[cfg(feature="solana")]
pub mod program;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Status {Pending, Released, Refunded}
pub fn can_settle(status:Status, buyer_signed:bool, deadline:i64, now:i64, refund:bool)->Result<(),String>{
 if status!=Status::Pending{return Err("Deal is already settled".into())}
 if !buyer_signed{return Err("Buyer signature required".into())}
 if refund && now<deadline{return Err("Refund becomes available at expiry".into())}
 if !refund && now>=deadline{return Err("Deal expired; refund instead".into())}
 Ok(())
}
#[derive(Clone,Serialize,Deserialize)]
pub struct Deal {pub id:u64,pub amount:u64,pub deadline:i64,pub status:Status}
#[derive(Clone,Serialize,Deserialize)]
pub struct Ledger {pub now:i64,pub buyer:u64,pub seller:u64,pub vaulted:u64,pub deals:Vec<Deal>,pub events:Vec<String>}
impl Default for Ledger {fn default()->Self{Self{now:0,buyer:1000,seller:0,vaulted:0,deals:vec![],events:vec![]}}}
impl Ledger {
 pub fn create(&mut self,amount:u64,duration:i64)->Result<u64,String>{
  if amount==0||amount>self.buyer||!(1..=2678400).contains(&duration){return Err("Invalid amount, balance or duration".into())}
  let deadline=self.now.checked_add(duration).ok_or("Time overflow")?;let id=self.deals.len() as u64+1;
  self.buyer-=amount;self.vaulted=self.vaulted.checked_add(amount).ok_or("Balance overflow")?;self.deals.push(Deal{id,amount,deadline,status:Status::Pending});self.events.push(format!("Created deal #{id}: {amount} demo lamports"));Ok(id)
 }
 pub fn settle(&mut self,id:u64,buyer_signed:bool,refund:bool)->Result<(),String>{
  let deal=self.deals.iter_mut().find(|d|d.id==id).ok_or("Unknown deal")?;can_settle(deal.status,buyer_signed,deal.deadline,self.now,refund)?;
  let destination=if refund{&mut self.buyer}else{&mut self.seller};let credited=destination.checked_add(deal.amount).ok_or("Balance overflow")?;
  self.vaulted-=deal.amount;*destination=credited;deal.status=if refund{Status::Refunded}else{Status::Released};self.events.push(format!("Deal #{id} {}",if refund{"refunded"}else{"released"}));Ok(())
 }
 pub fn advance(&mut self,seconds:i64)->Result<(),String>{if !(1..=2678400).contains(&seconds){return Err("Invalid time step".into())}self.now=self.now.checked_add(seconds).ok_or("Time overflow")?;Ok(())}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn signature_expiry_and_terminal_rules(){assert!(can_settle(Status::Pending,false,100,1,false).is_err());assert!(can_settle(Status::Pending,true,100,99,true).is_err());assert!(can_settle(Status::Pending,true,100,100,false).is_err());assert!(can_settle(Status::Pending,true,100,100,true).is_ok());assert!(can_settle(Status::Released,true,100,1,false).is_err());}
 #[test]fn release_conserves_balance_and_blocks_double_spend(){let mut l=Ledger::default();let id=l.create(250,60).unwrap();l.settle(id,true,false).unwrap();assert_eq!((l.buyer,l.seller,l.vaulted),(750,250,0));assert!(l.settle(id,true,false).is_err());assert_eq!(l.buyer+l.seller+l.vaulted,1000);}
 #[test]fn expiry_refunds_only_buyer(){let mut l=Ledger::default();let id=l.create(400,60).unwrap();assert!(l.settle(id,true,true).is_err());l.advance(60).unwrap();assert!(l.settle(id,false,true).is_err());l.settle(id,true,true).unwrap();assert_eq!((l.buyer,l.seller,l.vaulted),(1000,0,0));}
 #[test]fn invalid_deposit_keeps_ledger(){let mut l=Ledger::default();assert!(l.create(1001,60).is_err());assert!(l.create(0,60).is_err());assert_eq!(l.buyer,1000);assert!(l.deals.is_empty());}
}
