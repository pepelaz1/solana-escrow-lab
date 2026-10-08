//! Native SOL escrow using a pre-funded program-owned account signed at initialization.
use solana_program::{account_info::{next_account_info,AccountInfo},entrypoint::ProgramResult,program_error::ProgramError,pubkey::Pubkey,clock::Clock,rent::Rent,sysvar::Sysvar};
use crate::{can_settle,Status};
pub const SPACE:usize=82;
#[cfg(not(feature="no-entrypoint"))]
solana_program::entrypoint!(process_instruction);
pub fn process_instruction(program_id:&Pubkey,accounts:&[AccountInfo],data:&[u8])->ProgramResult{
 process_at(program_id,accounts,data,Clock::get()?.unix_timestamp,Rent::get()?.minimum_balance(SPACE))
}
pub fn process_at(program_id:&Pubkey,accounts:&[AccountInfo],instruction:&[u8],now:i64,rent_floor:u64)->ProgramResult{
 let it=&mut accounts.iter();let vault=next_account_info(it)?;let buyer=next_account_info(it)?;let seller=next_account_info(it)?;
 if vault.owner!=program_id{return Err(ProgramError::IncorrectProgramId)}
 if !vault.is_writable||vault.data_len()!=SPACE{return Err(ProgramError::InvalidAccountData)}
 if !buyer.is_signer{return Err(ProgramError::MissingRequiredSignature)}
 if vault.key==buyer.key||vault.key==seller.key||buyer.key==seller.key{return Err(ProgramError::InvalidArgument)}
 match instruction.first(){
  Some(0)=>{
   if instruction.len()!=17||!vault.is_signer{return Err(ProgramError::InvalidInstructionData)}
   let mut state=vault.try_borrow_mut_data()?;if state.iter().any(|b|*b!=0){return Err(ProgramError::AccountAlreadyInitialized)}
   let amount=u64::from_le_bytes(instruction[1..9].try_into().unwrap());let deadline=i64::from_le_bytes(instruction[9..17].try_into().unwrap());
   if amount==0||deadline<=now||deadline.checked_sub(now).ok_or(ProgramError::InvalidArgument)?>2678400{return Err(ProgramError::InvalidArgument)}
   if vault.lamports()<rent_floor.checked_add(amount).ok_or(ProgramError::ArithmeticOverflow)?{return Err(ProgramError::InsufficientFunds)}
   state[0]=1;state[1..33].copy_from_slice(buyer.key.as_ref());state[33..65].copy_from_slice(seller.key.as_ref());state[65..73].copy_from_slice(&amount.to_le_bytes());state[73..81].copy_from_slice(&deadline.to_le_bytes());state[81]=0;
  }
  Some(1)|Some(2)=>{
   if instruction.len()!=1{return Err(ProgramError::InvalidInstructionData)}
   let mut state=vault.try_borrow_mut_data()?;if state[0]!=1{return Err(ProgramError::UninitializedAccount)}
   if state[1..33]!=buyer.key.to_bytes()||state[33..65]!=seller.key.to_bytes(){return Err(ProgramError::InvalidArgument)}
   let status=match state[81]{0=>Status::Pending,1=>Status::Released,2=>Status::Refunded,_=>return Err(ProgramError::InvalidAccountData)};
   let amount=u64::from_le_bytes(state[65..73].try_into().unwrap());let deadline=i64::from_le_bytes(state[73..81].try_into().unwrap());let refund=instruction[0]==2;
   can_settle(status,true,deadline,now,refund).map_err(|_|ProgramError::InvalidArgument)?;
   let recipient=if refund{buyer}else{seller};if !recipient.is_writable{return Err(ProgramError::InvalidAccountData)}
   let remaining=vault.lamports().checked_sub(amount).ok_or(ProgramError::InsufficientFunds)?;if remaining<rent_floor{return Err(ProgramError::InsufficientFunds)}
   let credited=recipient.lamports().checked_add(amount).ok_or(ProgramError::ArithmeticOverflow)?;
   **vault.try_borrow_mut_lamports()?=remaining;**recipient.try_borrow_mut_lamports()?=credited;state[81]=if refund{2}else{1};
  }
  _=>return Err(ProgramError::InvalidInstructionData)
 }
 Ok(())
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn native_accounts_release_and_replay(){
  let program=Pubkey::new_unique();let vk=Pubkey::new_unique();let bk=Pubkey::new_unique();let sk=Pubkey::new_unique();let system=Pubkey::default();let(mut vl,mut bl,mut sl)=(1100,50,20);let mut vd=[0;SPACE];let mut bd=[];let mut sd=[];
  let vault=AccountInfo::new(&vk,true,true,&mut vl,&mut vd,&program,false,0);let buyer=AccountInfo::new(&bk,true,true,&mut bl,&mut bd,&system,false,0);let seller=AccountInfo::new(&sk,false,true,&mut sl,&mut sd,&system,false,0);let accounts=[vault,buyer,seller];
  let mut init=vec![0];init.extend(100u64.to_le_bytes());init.extend(60i64.to_le_bytes());process_at(&program,&accounts,&init,0,1000).unwrap();
  assert!(process_at(&program,&accounts,&[2],59,1000).is_err());process_at(&program,&accounts,&[1],59,1000).unwrap();assert_eq!(accounts[0].lamports(),1000);assert_eq!(accounts[2].lamports(),120);assert!(process_at(&program,&accounts,&[1],59,1000).is_err());
 }
 #[test]fn missing_signature_and_wrong_owner_rejected(){
  let program=Pubkey::new_unique();let other=Pubkey::new_unique();let keys=[Pubkey::new_unique(),Pubkey::new_unique(),Pubkey::new_unique()];let(mut a,mut b,mut c)=(1100,0,0);let mut data=[0;SPACE];let mut empty1=[];let mut empty2=[];
  let vault=AccountInfo::new(&keys[0],true,true,&mut a,&mut data,&other,false,0);let buyer=AccountInfo::new(&keys[1],false,true,&mut b,&mut empty1,&other,false,0);let seller=AccountInfo::new(&keys[2],false,true,&mut c,&mut empty2,&other,false,0);let mut accounts=[vault,buyer,seller];assert_eq!(process_at(&program,&accounts,&[1],0,1000),Err(ProgramError::IncorrectProgramId));
  accounts[0].owner=&program;assert_eq!(process_at(&program,&accounts,&[1],0,1000),Err(ProgramError::MissingRequiredSignature));
 }
 #[test]fn native_refund_at_expiry_checks_destination_and_signature(){
  let program=Pubkey::new_unique();let vk=Pubkey::new_unique();let bk=Pubkey::new_unique();let sk=Pubkey::new_unique();let system=Pubkey::default();let(mut vl,mut bl,mut sl)=(1200,50,20);let mut vd=[0;SPACE];let mut bd=[];let mut sd=[];
  let vault=AccountInfo::new(&vk,true,true,&mut vl,&mut vd,&program,false,0);let buyer=AccountInfo::new(&bk,true,true,&mut bl,&mut bd,&system,false,0);let seller=AccountInfo::new(&sk,false,true,&mut sl,&mut sd,&system,false,0);let mut accounts=[vault,buyer,seller];
  let mut init=vec![0];init.extend(200u64.to_le_bytes());init.extend(60i64.to_le_bytes());process_at(&program,&accounts,&init,0,1000).unwrap();
  assert!(process_at(&program,&accounts,&[1],60,1000).is_err());accounts[1].is_signer=false;assert!(process_at(&program,&accounts,&[2],60,1000).is_err());accounts[1].is_signer=true;accounts[1].is_writable=false;assert!(process_at(&program,&accounts,&[2],60,1000).is_err());accounts[1].is_writable=true;
  process_at(&program,&accounts,&[2],60,1000).unwrap();assert_eq!(accounts[0].lamports(),1000);assert_eq!(accounts[1].lamports(),250);assert_eq!(accounts[2].lamports(),20);assert!(process_at(&program,&accounts,&[2],60,1000).is_err());
 }

}
