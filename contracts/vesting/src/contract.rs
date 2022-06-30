#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, Addr, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo,
    Order, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;

use crate::helpers::{compute_position_response, compute_withdrawable};
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PositionResponse, QueryMsg, Schedule, VEST_DENOM,
};
use crate::state::{
    Position, OWNER, PENDING_OWNER, POSITIONS, TOTAL_VOTING_POWER, UNLOCK_SCHEDULE,
};

const CONTRACT_NAME: &str = "crates.io:mars-vesting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

//--------------------------------------------------------------------------------------------------
// Instantiation
//--------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    OWNER.save(deps.storage, &deps.api.addr_validate(&msg.owner)?)?;
    UNLOCK_SCHEDULE.save(deps.storage, &msg.unlock_schedule)?;
    TOTAL_VOTING_POWER.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new())
}

//--------------------------------------------------------------------------------------------------
// Executions
//--------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    let api = deps.api;
    match msg {
        ExecuteMsg::CreatePosition {
            user,
            vest_schedule,
        } => create_position(deps, info, api.addr_validate(&user)?, vest_schedule),
        ExecuteMsg::TransferOwnership {
            new_owner,
        } => transfer_ownership(deps, info.sender, api.addr_validate(&new_owner)?),
        ExecuteMsg::AcceptOwnership {} => accept_ownership(deps, info.sender),
        ExecuteMsg::Withdraw {} => withdraw(deps, env.block.time.seconds(), info.sender),
    }
}

pub fn create_position(
    deps: DepsMut,
    info: MessageInfo,
    user_addr: Addr,
    vest_schedule: Schedule,
) -> StdResult<Response> {
    // only owner can create allocations
    let owner_addr = OWNER.load(deps.storage)?;
    if info.sender != owner_addr {
        return Err(StdError::generic_err("only owner can create allocations"));
    }

    // must send exactly one coin
    if info.funds.len() != 1 {
        return Err(StdError::generic_err(
            format!("wrong number of coins: expecting 1, received {}", info.funds.len()),
        ));
    }

    // the coin must be the vesting coin
    let coin = &info.funds[0];
    if coin.denom != VEST_DENOM {
        return Err(StdError::generic_err(
            format!("wrong denom: expecting {}, received {}", VEST_DENOM, coin.denom),
        ));
    }

    // the amount must be greater than zero
    let total = coin.amount;
    if total.is_zero() {
        return Err(StdError::generic_err("wrong amount: must be greater than zero"));
    }

    POSITIONS.update(
        deps.storage,
        &user_addr,
        |position| {
            if position.is_some() {
                return Err(StdError::generic_err("user has a vesting position"));
            }
            Ok(Position {
                total,
                vest_schedule,
                withdrawn: Uint128::zero(),
            })
        },
    )?;

    TOTAL_VOTING_POWER.update(
        deps.storage,
        |tvp| -> StdResult<_> { Ok(tvp + total) },
    )?;

    let event = Event::new("mars/vesting/position_created")
        .add_attribute("user", user_addr)
        .add_attribute("total", total)
        .add_attribute("start_time", vest_schedule.start_time.to_string())
        .add_attribute("cliff", vest_schedule.cliff.to_string())
        .add_attribute("duration", vest_schedule.duration.to_string());

    Ok(Response::new().add_event(event))
}

pub fn withdraw(deps: DepsMut, time: u64, user_addr: Addr) -> StdResult<Response> {
    let unlock_schedule = UNLOCK_SCHEDULE.load(deps.storage)?;
    let mut position = POSITIONS.load(deps.storage, &user_addr)?;

    let (_, _, withdrawable) = compute_withdrawable(
        time,
        position.total,
        position.withdrawn,
        position.vest_schedule,
        unlock_schedule,
    );

    if withdrawable.is_zero() {
        return Err(StdError::generic_err("withdrawable amount is zero"));
    }

    position.withdrawn += withdrawable;
    POSITIONS.save(deps.storage, &user_addr, &position)?;

    TOTAL_VOTING_POWER.update(
        deps.storage,
        |tvp| -> StdResult<_> { Ok(tvp - withdrawable) },
    )?;

    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: user_addr.to_string(),
        amount: coins(withdrawable.u128(), VEST_DENOM),
    });

    let event = Event::new("mars/vesting/withdrawn")
        .add_attribute("user", user_addr)
        .add_attribute("timestamp", time.to_string())
        .add_attribute("withdrawable", withdrawable);

    Ok(Response::new().add_message(msg).add_event(event))
}

pub fn transfer_ownership(
    deps: DepsMut,
    sender_addr: Addr,
    new_owner_addr: Addr,
) -> StdResult<Response> {
    let owner_addr = OWNER.load(deps.storage)?;
    if sender_addr != owner_addr {
        return Err(StdError::generic_err("only owner can proposal ownership transfers"));
    }

    PENDING_OWNER.save(deps.storage, &new_owner_addr)?;

    let event = Event::new("mars/vesting/ownership_transfer_proposed")
        .add_attribute("current_owner", owner_addr)
        .add_attribute("pending_owner", new_owner_addr);

    Ok(Response::new().add_event(event))
}

pub fn accept_ownership(
    deps: DepsMut,
    sender_addr: Addr,
) -> StdResult<Response> {
    let pending_owner_addr = PENDING_OWNER.load(deps.storage)?;
    if sender_addr != pending_owner_addr {
        return Err(StdError::generic_err("only pending owner an accept ownership"));
    }

    let previous_owner_addr = OWNER.load(deps.storage)?;
    OWNER.save(deps.storage, &pending_owner_addr)?;

    PENDING_OWNER.remove(deps.storage);

    let event = Event::new("mars/vesting/ownership_transfer_completed")
        .add_attribute("previous_owner", previous_owner_addr)
        .add_attribute("new_owner", pending_owner_addr);

    Ok(Response::new().add_event(event))
}

//--------------------------------------------------------------------------------------------------
// Queries
//--------------------------------------------------------------------------------------------------

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let api = deps.api;
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::TotalVotingPower {} => to_binary(&query_total_voting_power(deps)?),
        QueryMsg::VotingPower {
            user,
        } => to_binary(&query_voting_power(deps, api.addr_validate(&user)?)?),
        QueryMsg::Position {
            user,
        } => to_binary(&query_position(deps, env.block.time.seconds(), api.addr_validate(&user)?)?),
        QueryMsg::Positions {
            start_after,
            limit,
        } => to_binary(&query_positions(deps, env.block.time.seconds(), start_after, limit)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    Ok(ConfigResponse {
        owner: OWNER.load(deps.storage)?.into(),
        pending_owner: PENDING_OWNER.may_load(deps.storage)?.map(String::from),
        unlock_schedule: UNLOCK_SCHEDULE.load(deps.storage)?,
    })
}

pub fn query_total_voting_power(deps: Deps) -> StdResult<Uint128> {
    TOTAL_VOTING_POWER.load(deps.storage)
}

pub fn query_voting_power(deps: Deps, user_addr: Addr) -> StdResult<Uint128> {
    match POSITIONS.may_load(deps.storage, &user_addr) {
        Ok(Some(position)) => Ok(position.total - position.withdrawn),
        Ok(None) => Ok(Uint128::zero()),
        Err(err) => Err(err),
    }
}

pub fn query_position(deps: Deps, time: u64, user_addr: Addr) -> StdResult<PositionResponse> {
    let unlock_schedule = UNLOCK_SCHEDULE.load(deps.storage)?;
    let position = POSITIONS.load(deps.storage, &user_addr)?;

    Ok(compute_position_response(time, user_addr, &position, unlock_schedule))
}

pub fn query_positions(
    deps: Deps,
    time: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<PositionResponse>> {
    let unlock_schedule = UNLOCK_SCHEDULE.load(deps.storage)?;

    let addr: Addr;
    let start = match &start_after {
        Some(addr_str) => {
            addr = deps.api.addr_validate(addr_str)?;
            Some(Bound::exclusive(&addr))
        }
        None => None,
    };

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    POSITIONS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|res| {
            let (user_addr, position) = res?;
            Ok(compute_position_response(time, user_addr, &position, unlock_schedule))
        })
        .collect()
}
