mod utils;

use std::cmp::max;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, Gas, promise_result_as_success, serde_json::json, AccountId, Promise, Balance, CryptoHash, BorshStorageKey, PromiseResult};
use near_sdk::collections::{LookupMap, UnorderedMap, UnorderedSet, Vector};
use std::collections::HashMap;
use near_sdk::serde::{Deserialize, Serialize};

use near_sdk::ext_contract;
use near_contract_standards::non_fungible_token::{hash_account_id, TokenId};
use near_sdk::json_types::{U128, U64};
use crate::utils::{delete_from_vector_by_offer_id, delete_from_vector_by_uid};


#[ext_contract(nft_contract)]
trait ExtContract {
    fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>,
    );
}

#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        nft_uid: TokenUID,
        price: U128,
    );
}

const GAS_FOR_NFT_TRANSFER: Gas = Gas(20_000_000_000_000);
const BASE_GAS: Gas = Gas(5_000_000_000_000);
const GAS_FOR_ROYALTIES: Gas = Gas(BASE_GAS.0 * 10u64);
const NO_DEPOSIT: Balance = 0;

const TREASURY_FEE: u128 = 200;
// 0.02
const TREASURY_ID: &str = "treasury1.near";
const CONTRACT_ID: &str = "mjol.near";

const UID_DELIMITER: &str = ":";

const OFFER_PREFIX: &str = "offer";
const OFFER_DELIMITER: &str = "-";

pub type Payout = HashMap<AccountId, U128>;
pub type TokenUID = String;


#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Listings,
    TokenUIDToData,
    TokenUIDsByOwner,
    TokenUIDsByOwnerInner { account_id_hash: CryptoHash },
    Whitelist,
    Offers,
    OffersInner { account_id_hash: CryptoHash },
    OfferByAccountId,
    OfferByAccountIdInner { account_id_hash: CryptoHash },
    OfferIdToAccountId,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PayoutStruct {
    pub payout: Payout,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SiteMetadata {
    pub name: String,
    pub nft_link: String,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ApprovedNFT {
    pub contract_id: AccountId,
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub title: String,
    pub description: Option<String>,
    pub copies: U64,
    pub media_url: String,
    pub reference_url: String,
    pub mint_site: SiteMetadata,
    pub price: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketArgs {
    pub json_nft: ApprovedNFT,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenData {
    pub owner_id: AccountId,
    pub nft_contract_id: AccountId,
    pub token_id: TokenId,
    pub price: Balance,
    pub approval_id: u64,
}

type OfferId = String;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Offer {
    pub token_uid: TokenUID,
    pub offer_id: OfferId,
    pub price: Balance,
    pub buyer_id: AccountId,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    listings: Vector<TokenUID>,
    uid_to_data: UnorderedMap<TokenUID, TokenData>,
    user_to_uids: UnorderedMap<AccountId, Vector<TokenUID>>,
    whitelist: UnorderedSet<AccountId>,
    offers: LookupMap<TokenUID, Vector<Offer>>,
    offers_by_account_id: LookupMap<AccountId, Vector<Offer>>,
    offer_id_to_account_id: LookupMap<OfferId, AccountId>,
    offer_counter: u128,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketData {
    pub tokens: Vec<TokenData>,
    pub has_next_batch: bool,
    pub total_count: u64,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            listings: Vector::new(StorageKey::Listings),
            uid_to_data: UnorderedMap::new(StorageKey::TokenUIDToData),
            user_to_uids: UnorderedMap::new(StorageKey::TokenUIDsByOwner),
            whitelist: UnorderedSet::new(StorageKey::Whitelist),
            offers: LookupMap::new(StorageKey::Offers),
            offers_by_account_id: LookupMap::new(StorageKey::OfferByAccountId),
            offer_id_to_account_id: LookupMap::new(StorageKey::OfferIdToAccountId),
            offer_counter: 0,
        }
    }
}

#[near_bindgen]
impl Contract {
    #[init(ignore_state)]
    pub fn new() -> Self {
        assert_eq!(env::predecessor_account_id().to_string(), CONTRACT_ID);
        Self {
            listings: Vector::new(StorageKey::Listings),
            uid_to_data: UnorderedMap::new(StorageKey::TokenUIDToData),
            user_to_uids: UnorderedMap::new(StorageKey::TokenUIDsByOwner),
            whitelist: UnorderedSet::new(StorageKey::Whitelist),
            offers: LookupMap::new(StorageKey::Offers),
            offers_by_account_id: LookupMap::new(StorageKey::OfferByAccountId),
            offer_id_to_account_id: LookupMap::new(StorageKey::OfferIdToAccountId),
            offer_counter: 0,
        }
    }

    #[payable]
    pub fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    ) {
        assert_eq!(env::signer_account_id(), owner_id, "You are not the owner of the NFT");

        let nft_contract_id = env::predecessor_account_id();
        assert_ne!(env::signer_account_id(), nft_contract_id, "Cross contract call awaited");

        let MarketArgs {
            json_nft
        } = near_sdk::serde_json::from_str(&msg).expect("Not valid MarketArgs");

        let price = json_nft.price;


        let new_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);

        assert!(self.uid_to_data.get(&new_uid.clone()).is_none(),
                "This NFT is already on the market");

        // update users listing info
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&owner_id.clone())
            .unwrap_or_else(|| {
                Vector::new(StorageKey::TokenUIDsByOwnerInner {
                    account_id_hash: hash_account_id(&owner_id.clone())
                }
                )
            });
        cur_users_token_uids.push(&new_uid.clone());
        self
            .user_to_uids
            .insert(&owner_id.clone(), &cur_users_token_uids);

        // add new listing to all listings
        self.listings.push(&new_uid.clone());

        // add new uid -> TokenData
        self.uid_to_data.insert(&new_uid.clone(), &TokenData {
            owner_id: owner_id.clone(),
            nft_contract_id: nft_contract_id.clone(),
            token_id: token_id.clone(),
            price: price.0,
            approval_id: approval_id.clone(),
        });

        env::log_str(&json!({
        "type": "nft_on_approve",
        "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "approval_id": U64::from(approval_id),
                "json_nft": json_nft
        }
        }).to_string());
    }

    #[payable]
    pub fn remove_from_market(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let owner_id = nft_data.owner_id.clone();
        let caller_id = env::signer_account_id();

        assert_eq!(owner_id, caller_id);

        self.remove_nft(owner_id, nft_uid);

        env::log_str(&json!({
            "type": "remove_from_market",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "owner_id": nft_data.owner_id,
                "approval_id": U64::from(nft_data.approval_id),
                "price": U128::from(nft_data.price)
            }
        }).to_string());
    }

    #[payable]
    pub fn buy_with_payouts(
        &mut self,
        nft_contract_id: AccountId,
        token_id: TokenId,
    ) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let nft_data = self.uid_to_data.get(&nft_uid.clone())
            .expect("NFT does not exist.");

        let cur_approval_id: u64 = nft_data.approval_id.clone();
        let cur_price: U128 = U128::from(nft_data.price.clone());
        let seller_id = nft_data.owner_id.clone();
        let buyer_id = env::signer_account_id();


        assert_eq!(U128::from(env::attached_deposit()), cur_price);

        self.buy_inner(nft_contract_id.clone(),
                       token_id.clone(),
                       cur_approval_id,
                       cur_price,
                       seller_id,
                       buyer_id);

        env::log_str(&json!({
            "type": "buy_with_payouts",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "owner_id": nft_data.owner_id.clone(),
                "approval_id": U64::from(nft_data.approval_id.clone()),
                "price": U128::from(nft_data.price.clone())
            }
        }).to_string());
    }

    #[payable]
    pub fn add_offer(
        &mut self,
        token_id: TokenId,
        nft_contract_id: AccountId,
    ) {
        let buyer_id = env::signer_account_id();
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let token_data = self.uid_to_data.get(&nft_uid.clone()).expect("No such token");

        assert_ne!(token_data.owner_id, buyer_id);

        let price = env::attached_deposit();

        let next_offer_id = self.offer_counter;
        self.offer_counter += 1;

        let offer_id: OfferId = format!("{}{}{}", OFFER_PREFIX, OFFER_DELIMITER, next_offer_id);

        let offer = Offer {
            token_uid: nft_uid.clone(),
            offer_id: offer_id.clone(),
            price: price.clone(),
            buyer_id: buyer_id.clone(),
        };

        let mut token_offers = self
            .offers
            .get(&nft_uid.clone())
            .unwrap_or_else(
                || Vector::new(StorageKey::OffersInner {
                    account_id_hash: hash_account_id(&buyer_id.clone())
                })
            );

        let mut buyer_offers = self
            .offers_by_account_id
            .get(&buyer_id.clone())
            .unwrap_or_else(
                || Vector::new(StorageKey::OfferByAccountIdInner {
                    account_id_hash: hash_account_id(&buyer_id.clone())
                })
            );

        token_offers.push(&offer.clone());
        buyer_offers.push(&offer.clone());

        self.offers.insert(&nft_uid.clone(), &token_offers);
        self.offers_by_account_id.insert(&buyer_id.clone(), &buyer_offers);
        self.offer_id_to_account_id.insert(&offer_id.clone(), &buyer_id.clone());

        Promise::new(AccountId::new_unchecked(CONTRACT_ID.to_string()))
            .transfer(price.clone());

        env::log_str(&json!({
            "type": "add_offer",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "offer_id": offer_id,
                "buyer_id": buyer_id,
                "price": U128::from(price)
            }
        }).to_string());
    }

    #[payable]
    pub fn accept_offer(
        &mut self,
        token_id: TokenId,
        nft_contract_id: AccountId,
        offer_id: OfferId,
    ) {
        let seller_id = env::predecessor_account_id();
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        let token_data = self.uid_to_data.get(&nft_uid.clone()).expect("No such token");

        assert_eq!(token_data.owner_id, seller_id);

        let offer = self
            .offers
            .get(&nft_uid.clone())
            .expect("No offers for token.")
            .iter()
            .find(|x| x.offer_id == offer_id)
            .expect("No such offer.");

        assert_eq!(offer.buyer_id, seller_id);

        self.buy_inner(nft_contract_id.clone(),
                       token_id.clone(),
                       token_data.approval_id.clone(),
                       U128::from(offer.price),
                       seller_id,
                       offer.buyer_id.clone());

        env::log_str(&json!({
            "type": "accept_offer",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "offer_id": offer_id,
                "buyer_id": offer.buyer_id,
                "price": U128::from(offer.price)
            }
        }).to_string());
    }

    #[payable]
    pub fn delete_offer(
        &mut self,
        token_id: TokenId,
        nft_contract_id: AccountId,
        offer_id: OfferId,
    ) {
        let buyer_id = env::predecessor_account_id();
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);
        self.uid_to_data.get(&nft_uid.clone()).expect("No such token");

        let offers_owner = self.offer_id_to_account_id.get(&offer_id.clone())
            .expect("No such offer.");

        assert_eq!(buyer_id, offers_owner);

        let mut offers = self
            .offers
            .get(&nft_uid.clone())
            .expect("No offers for token.");

        delete_from_vector_by_offer_id(&mut offers, &offer_id.clone())
            .expect("No such offer for token.");

        self.offers.insert(&nft_uid.clone(), &offers);

        let mut buyer_offers = self
            .offers_by_account_id
            .get(&buyer_id.clone())
            .expect("No offers for this account id.");

        let offer = delete_from_vector_by_offer_id(&mut buyer_offers, &offer_id.clone())
            .expect("No such offer for token.");

        self
            .offers_by_account_id
            .insert(&buyer_id.clone(), &buyer_offers);

        self.offer_id_to_account_id.remove(&offer_id.clone());

        Promise::new(AccountId::new_unchecked(buyer_id.to_string()))
            .transfer(offer.price);

        env::log_str(&json!({
            "type": "delete_offer",
            "data": {
                "nft_contract_id": nft_contract_id,
                "token_id": token_id,
                "offer_id": offer_id,
                "buyer_id": offer.buyer_id,
                "price": U128::from(offer.price)
            }
        }).to_string());
    }

    pub fn get_nfts(self, from: u64, limit: u64) -> MarketData {
        let size = self.listings.len() as u64;
        let mut res = vec![];
        if from >= size {
            return MarketData {
                tokens: res,
                has_next_batch: false,
                total_count: size,
            };
        }
        let real_to = (size - from) as usize;
        let real_from = max(real_to as i64 - limit as i64, 0 as i64) as usize;

        for i in (real_from..real_to).rev() {
            res.push(self.uid_to_data
                .get(&self.listings.get(i as u64).unwrap()).unwrap())
        }


        MarketData {
            tokens: res,
            has_next_batch: real_from > 0,
            total_count: size,
        }
    }

    pub fn get_user_nfts(self, owner_id: AccountId) -> Vec<TokenData> {
        let all_uids = self.user_to_uids
            .get(&owner_id.clone());

        return if let Some(uids) = all_uids {
            uids.iter().map(|x| {
                self.uid_to_data.get(&x.clone()).unwrap()
            }).collect()
        } else {
            vec![]
        };
    }

    pub fn get_nft_price(self, token_uid: TokenUID) -> Option<u128> {
        let token = self.uid_to_data.get(&token_uid);
        if let Some(token) = token {
            return Some(token.price);
        }
        None
    }

    pub fn get_whitelist(&self) -> Vec<AccountId> {
        self.whitelist.to_vec()
    }

    pub fn get_created_offers_for_account_id(self, account_id: AccountId) -> Option<Vec<Offer>> {
        self.offers_by_account_id.get(&account_id).map(|x| x.to_vec())
    }

    pub fn get_offers_by_token_uid(self, token_uid: TokenUID) -> Option<Vec<Offer>> {
        self.offers.get(&token_uid).map(|x| x.to_vec())
    }

    #[private]
    pub fn resolve_purchase(
        &mut self,
        buyer_id: AccountId,
        seller_id: AccountId,
        nft_uid: TokenUID,
        price: U128,
    ) {
        assert_eq!(env::promise_results_count(), 1);
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => env::panic_str("NFT Transfer failed. Try again."),
            PromiseResult::Successful(_) => env::log_str("Transfer OK.")
        }

        self.remove_nft(seller_id.clone(), nft_uid.clone());

        let payout_option = promise_result_as_success().and_then(|value| {

            // If Payout is struct with payout field than get it
            let res = near_sdk::serde_json::from_slice::<PayoutStruct>(&value);
            if res.is_ok() {
                res.ok().and_then(|payout| {
                    Contract::check_payouts(price, payout.payout)
                })
            } else {
                near_sdk::serde_json::from_slice::<Payout>(&value).ok().and_then(|payout| {
                    Contract::check_payouts(price, payout)
                })
            }
        });

        let payout = if let Some(payout_option) = payout_option {
            payout_option
        } else {
            let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;
            Promise::new(seller_id.clone())
                .transfer(price.0 - treasury_fee);
            Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string()))
                .transfer(treasury_fee);

            env::log_str(
                &json!({
                    "type": "resolve_purchase_force",
                    "data": {
                        "price": U128::from(price),
                        "buyer_id": buyer_id,
                        "seller_id": seller_id,
                        "nft_uid": nft_uid
                    }
                }).to_string()
            );
            return;
        };

        // 2% fee for treasury
        let treasury_fee = price.0 * TREASURY_FEE / 10_000u128;

        for (receiver_id, amount) in payout {
            if receiver_id == seller_id {
                Promise::new(receiver_id).transfer(amount.0 - treasury_fee);
                Promise::new(AccountId::new_unchecked(TREASURY_ID.to_string())).transfer(treasury_fee);
            } else {
                Promise::new(receiver_id).transfer(amount.0);
            }
        }
        env::log_str(
            &json!({
                    "type": "resolve_purchase",
                    "data": {
                        "price": U128::from(price),
                        "buyer_id": buyer_id,
                        "seller_id": seller_id,
                        "nft_uid": nft_uid
                    }
                }).to_string()
        );
    }


    #[private]
    pub fn add_to_whitelist(&mut self, contract_id: AccountId) {
        self.whitelist.insert(&contract_id);
    }

    #[private]
    pub fn remove_from_whitelist(&mut self, contract_id: AccountId) -> bool {
        self.whitelist.remove(&contract_id)
    }

    #[init(ignore_state)]
    #[private]
    pub fn migrate() -> Self {
        #[derive(BorshDeserialize)]
        struct Old {
            listings: Vector<TokenUID>,
            uid_to_data: UnorderedMap<TokenUID, TokenData>,
            user_to_uids: UnorderedMap<AccountId, Vector<TokenUID>>,
            whitelist: UnorderedSet<AccountId>,
        }

        let prev_state: Old = env::state_read().expect("No such state.");

        Self {
            listings: prev_state.listings,
            uid_to_data: prev_state.uid_to_data,
            user_to_uids: prev_state.user_to_uids,
            whitelist: prev_state.whitelist,
            offers: LookupMap::new(StorageKey::Offers),
            offers_by_account_id: LookupMap::new(StorageKey::OfferByAccountId),
            offer_id_to_account_id: LookupMap::new(StorageKey::OfferIdToAccountId),
            offer_counter: 0,
        }
    }

    fn check_payouts(
        price: U128,
        payout: Payout,
    ) -> Option<Payout> {
        let mut remainder = price.0;
        for &value in payout.values() {
            remainder = remainder.checked_sub(value.0)?;
        }
        if remainder == 0 || remainder == 1 {
            Some(payout)
        } else {
            None
        }
    }

    fn remove_nft(&mut self, owner_id: AccountId, nft_uid: TokenUID) {
        // delete from owner's listings
        let mut cur_users_token_uids = self
            .user_to_uids
            .get(&owner_id.clone())
            .unwrap_or_else(|| {
                Vector::new(StorageKey::TokenUIDsByOwnerInner {
                    account_id_hash: hash_account_id(&owner_id.clone())
                }
                )
            });
        assert!(delete_from_vector_by_uid(&mut cur_users_token_uids, &nft_uid.clone()).is_some());
        self
            .user_to_uids
            .insert(&owner_id.clone(), &cur_users_token_uids);

        // delete from all listings
        assert!(delete_from_vector_by_uid(&mut self.listings, &nft_uid.clone()).is_some());

        // delete info about NFT
        assert!(self.uid_to_data.remove(&nft_uid.clone()).is_some());
        // delete offers
        if let Some(offers) = self.offers.remove(&nft_uid.clone()) {
            offers
                .to_vec()
                .iter()
                .for_each(|offer| {
                    let buyer_offers_opt = self
                        .offers_by_account_id
                        .get(&offer.buyer_id.clone());

                    self.offer_id_to_account_id.remove(&offer.offer_id.clone());

                    if let Some(mut buyer_offers) = buyer_offers_opt {
                        let res = delete_from_vector_by_offer_id(&mut buyer_offers, &offer.offer_id.clone());
                        self
                            .offers_by_account_id
                            .insert(&offer.buyer_id.clone(), &buyer_offers);
                        if let Some(return_offer) = res {
                            Promise::new(
                                AccountId::new_unchecked(return_offer.buyer_id.to_string())
                            )
                                .transfer(return_offer.price.clone());
                        }
                    }
                })
        };
    }

    #[payable]
    fn buy_inner(&mut self,
                 nft_contract_id: AccountId,
                 token_id: TokenId,
                 cur_approval_id: u64,
                 cur_price: U128,
                 seller_id: AccountId,
                 buyer_id: AccountId) {
        let nft_uid: TokenUID = format!("{}{}{}", nft_contract_id, UID_DELIMITER, token_id);

        assert_ne!(seller_id, buyer_id);

        nft_contract::nft_transfer_payout(
            buyer_id.clone(),      // receiver_id: ValidAccountId,
            token_id.clone(),      // token_id: TokenId,
            Some(cur_approval_id), // approval_id: Option<u64>,
            Some(cur_price),       // balance: Option<U128>,
            Some(10u32),           // max_len_payout: Option<u32>
            nft_contract_id.clone(),
            1,
            GAS_FOR_NFT_TRANSFER,
        ).then(ext_self::resolve_purchase(
            buyer_id,
            seller_id,
            nft_uid,
            cur_price,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_ROYALTIES,
        ));
    }
}
