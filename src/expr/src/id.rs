// Copyright Materialize, Inc. and contributors. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use std::fmt;
use std::str::FromStr;

use anyhow::{anyhow, Error};
use serde::{Deserialize, Serialize};

// The `Arbitrary` impls are only used during testing and we gate them
// behind `cfg(feature = "test-utils")`, so `proptest` can remain a dev-dependency.
// See https://github.com/MaterializeInc/materialize/pull/11717.
#[cfg(feature = "test-utils")]
use proptest_derive::Arbitrary;

/// An opaque identifier for a dataflow component. In other words, identifies
/// the target of a [`MirRelationExpr::Get`](crate::MirRelationExpr::Get).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "test-utils", derive(Arbitrary))]
pub enum Id {
    /// An identifier that refers to a local component of a dataflow.
    Local(LocalId),
    /// An identifier that refers to a global dataflow.
    Global(GlobalId),
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Id::Local(id) => id.fmt(f),
            Id::Global(id) => id.fmt(f),
        }
    }
}

/// The identifier for a local component of a dataflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "test-utils", derive(Arbitrary))]
pub struct LocalId(pub(crate) u64);

impl LocalId {
    /// Constructs a new local identifier. It is the caller's responsibility
    /// to provide a unique `v`.
    pub fn new(v: u64) -> LocalId {
        LocalId(v)
    }
}

impl fmt::Display for LocalId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "l{}", self.0)
    }
}

/// The identifier for a global dataflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "test-utils", derive(Arbitrary))]
pub enum GlobalId {
    /// System namespace.
    System(u64),
    /// User namespace.
    User(u64),
    /// Transient namespace.
    Transient(u64),
    /// Dummy id for query being explained
    Explain,
}

impl GlobalId {
    /// Reports whether this ID is in the system namespace.
    pub fn is_system(&self) -> bool {
        matches!(self, GlobalId::System(_))
    }

    /// Reports whether this ID is in the user namespace.
    pub fn is_user(&self) -> bool {
        matches!(self, GlobalId::User(_))
    }

    /// Reports whether this ID is in the transient namespace.
    pub fn is_transient(&self) -> bool {
        matches!(self, GlobalId::Transient(_))
    }
}

impl FromStr for GlobalId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 2 {
            return Err(anyhow!("couldn't parse id {}", s));
        }
        let val: u64 = s[1..].parse()?;
        match s.chars().next().unwrap() {
            's' => Ok(GlobalId::System(val)),
            'u' => Ok(GlobalId::User(val)),
            't' => Ok(GlobalId::Transient(val)),
            _ => Err(anyhow!("couldn't parse id {}", s)),
        }
    }
}

impl fmt::Display for GlobalId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GlobalId::System(id) => write!(f, "s{}", id),
            GlobalId::User(id) => write!(f, "u{}", id),
            GlobalId::Transient(id) => write!(f, "t{}", id),
            GlobalId::Explain => write!(f, "Explained Query"),
        }
    }
}

/// Unique identifier for an instantiation of a source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SourceInstanceId {
    /// The ID of the source, shared across all instances.
    pub source_id: GlobalId,
    /// The ID of the timely dataflow containing this instantiation of this
    /// source.
    pub dataflow_id: usize,
}

impl fmt::Display for SourceInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.source_id, self.dataflow_id)
    }
}

/// Unique identifier for each part of a whole source.
///     Kafka -> partition
///     None -> sources that have no notion of partitioning (e.g file sources)
#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "test-utils", derive(Arbitrary))]
pub enum PartitionId {
    Kafka(i32),
    None,
}

impl fmt::Display for PartitionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PartitionId::Kafka(id) => write!(f, "{}", id),
            PartitionId::None => write!(f, "none"),
        }
    }
}

impl From<&PartitionId> for Option<String> {
    fn from(pid: &PartitionId) -> Option<String> {
        match pid {
            PartitionId::None => None,
            _ => Some(pid.to_string()),
        }
    }
}

impl FromStr for PartitionId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(PartitionId::None),
            s => {
                let val: i32 = s.parse()?;
                Ok(PartitionId::Kafka(val))
            }
        }
    }
}
