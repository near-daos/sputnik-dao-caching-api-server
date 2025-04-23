const fs = require("fs");
const { KeyPair, connect, utils } = require("near-api-js");

const accountsNumber = 4;

const config = {
  networkId: "sandbox",
  nodeUrl: "http://localhost:3030",
  keyPath: "../sandbox/validator_key.json",
  masterAccount: "test.near",
  smartContractPath: "../sputnikdao2.wasm",
  accountsFile: "created-accounts.json",
  gasLimit: "300000000000000",
};

const proposalDefaults = {
  description: "test",
  amount: "0.01", // in NEAR
};

function log(message) {
  console.log(`[${new Date().toISOString()}] ${message}`);
}

function loadPrivateKey() {
  try {
    const keyFile = fs.readFileSync(config.keyPath);
    return JSON.parse(keyFile.toString());
  } catch (error) {
    console.error(`Error loading private key: ${error.message}`);
    process.exit(1);
  }
}

function loadAccounts() {
  if (!fs.existsSync(config.accountsFile)) {
    console.error(`No accounts found. Please run 'init' command first.`);
    return null;
  }

  const accounts = JSON.parse(fs.readFileSync(config.accountsFile));
  if (accounts.length === 0) {
    console.error("No accounts available.");
    return null;
  }

  return accounts;
}

async function connectToNear(accountId, privateKey) {
  try {
    let keyPair;
    if (!privateKey) {
      const privateKeyData = loadPrivateKey();
      keyPair = KeyPair.fromString(
        privateKeyData.secret_key || privateKeyData.private_key,
      );
    } else {
      keyPair = KeyPair.fromString(privateKey);
    }

    const near = await connect({
      deps: {
        keyStore: { getKey: async () => keyPair },
      },
      networkId: config.networkId,
      nodeUrl: config.nodeUrl,
    });

    return {
      near,
      account: await near.account(accountId),
    };
  } catch (error) {
    console.error(`Error connecting to NEAR: ${error.message}`);
    return null;
  }
}

async function initAccounts() {
  try {
    const { account: masterAccount } = await connectToNear(
      config.masterAccount,
    );
    if (!masterAccount) return false;

    const createdAccounts = [];
    for (let i = 0; i < accountsNumber; i++) {
      const keyPair = KeyPair.fromRandom("ed25519");
      const accountId = `account-${i}.test.near`;

      log(`Creating account: ${accountId}`);
      try {
        await masterAccount.createAccount(
          accountId,
          keyPair.publicKey,
          utils.format.parseNearAmount("5"), // Initial balance
        );

        createdAccounts.push({
          accountId,
          publicKey: keyPair.publicKey.toString(),
          privateKey: `ed25519:${keyPair.secretKey}`,
        });
        log(`Successfully created account: ${accountId}`);
      } catch (error) {
        console.error(`Error creating account ${accountId}: ${error.message}`);
      }
    }

    fs.writeFileSync(
      config.accountsFile,
      JSON.stringify(createdAccounts, null, 2),
    );
    log(`Successfully created ${createdAccounts.length} accounts`);
    return createdAccounts.length > 0;
  } catch (error) {
    console.error(`Account initialization error: ${error.message}`);
    return false;
  }
}

async function deployContract() {
  try {
    const accounts = loadAccounts();
    if (!accounts) return false;

    const firstAccount = accounts[0];
    log(`Deploying smart contract to: ${firstAccount.accountId}`);

    const contractCode = fs.readFileSync(config.smartContractPath);
    const { account } = await connectToNear(
      firstAccount.accountId,
      firstAccount.privateKey,
    );
    if (!account) return false;

    const result = await account.deployContract(contractCode);
    log(
      `Successfully deployed contract. Transaction hash: ${result.transaction.hash}`,
    );
    return true;
  } catch (error) {
    console.error(`Deployment error: ${error.message}`);
    return false;
  }
}

async function initializeContract() {
  try {
    const accounts = loadAccounts();
    if (!accounts) return false;

    const firstAccount = accounts[0];
    const { account } = await connectToNear(
      firstAccount.accountId,
      firstAccount.privateKey,
    );
    if (!account) return false;

    // Extract all account IDs for the policy parameter
    const policyAccountIds = accounts.map((acc) => acc.accountId);
    const initParams = {
      config: {
        name: "test",
        purpose: "test",
        metadata: "",
      },
      policy: policyAccountIds,
    };

    log(`Initializing contract on: ${firstAccount.accountId}`);
    const result = await account.functionCall({
      contractId: firstAccount.accountId,
      methodName: "new",
      args: initParams,
      gas: config.gasLimit,
      attachedDeposit: "0",
    });

    log(`Contract initialized. Transaction hash: ${result.transaction.hash}`);
    return true;
  } catch (error) {
    console.error(`Contract initialization error: ${error.message}`);
    return false;
  }
}

async function addProposal() {
  try {
    const accounts = loadAccounts();
    if (!accounts) return false;

    const contractAccount = accounts[0];
    const receiverAccount = accounts[2];

    const { account: rootAccount } = await connectToNear(config.masterAccount);
    if (!rootAccount) return false;

    log(`Adding proposal to contract: ${contractAccount.accountId}`);

    const proposalInput = {
      proposal: {
        description: proposalDefaults.description,
        kind: {
          Transfer: {
            token_id: "", // near base token
            receiver_id: receiverAccount.accountId,
            amount: utils.format.parseNearAmount(proposalDefaults.amount),
          },
        },
      },
    };

    const result = await rootAccount.functionCall({
      contractId: contractAccount.accountId,
      methodName: "add_proposal",
      args: proposalInput,
      gas: config.gasLimit,
      attachedDeposit: utils.format.parseNearAmount("1"),
    });

    const proposalId = JSON.parse(
      Buffer.from(result.status.SuccessValue, "base64").toString(),
    );

    log(`Proposal added successfully. ID: ${proposalId}`);
    return proposalId;
  } catch (error) {
    console.error(`Adding proposal error: ${error.message}`);
    return false;
  }
}

async function voteOnProposal(proposalId, votersAmount) {
  try {
    const accounts = loadAccounts();
    if (!accounts) return false;

    // Validate and adjust votersAmount if needed
    const maxVoters = accounts.length - 1;
    if (votersAmount > maxVoters) {
      console.warn(
        `Requested ${votersAmount} voters, but only ${maxVoters} available.`,
      );
      votersAmount = maxVoters;
    }

    const contractAccount = accounts[0];
    const receiverAccount = accounts[2];
    const voterAccounts = accounts.slice(1, votersAmount + 1);

    log(
      `Voting on proposal ${proposalId} with ${voterAccounts.length} accounts`,
    );

    const proposalKind = {
      Transfer: {
        token_id: "",
        receiver_id: receiverAccount.accountId,
        amount: utils.format.parseNearAmount(proposalDefaults.amount),
      },
    };

    let successfulVotes = 0;
    for (const voterAccount of voterAccounts) {
      try {
        const { account } = await connectToNear(
          voterAccount.accountId,
          voterAccount.privateKey,
        );
        if (!account) continue;

        const voteInput = {
          id: Number(proposalId),
          action: "VoteApprove",
          proposal: proposalKind,
        };

        const result = await account.functionCall({
          contractId: contractAccount.accountId,
          methodName: "act_proposal",
          args: voteInput,
          gas: config.gasLimit,
          attachedDeposit: "0",
        });

        log(`Vote successful from ${voterAccount.accountId}`);
        successfulVotes++;
      } catch (error) {
        console.error(
          `Error voting from ${voterAccount.accountId}: ${error.message}`,
        );
      }
    }

    log(
      `Voting completed. Successful votes: ${successfulVotes}/${voterAccounts.length}`,
    );
    return successfulVotes > 0;
  } catch (error) {
    console.error(`Voting error: ${error.message}`);
    return false;
  }
}

const commands = {
  init: async () => {
    log("Initializing and creating accounts...");
    await initAccounts();
  },
  deploy: async () => {
    log(`Deploying smart contract from: ${config.smartContractPath}`);
    await deployContract();
  },
  load: async () => {
    log("Initializing deployed contract...");
    await initializeContract();
  },
  add_proposal: async () => {
    log("Adding a proposal to the contract...");
    await addProposal();
  },
  vote: async (proposalId, votersAmountStr) => {
    if (!proposalId) {
      console.error("Missing proposal ID parameter");
      console.log("Usage: node script.js vote <proposalId> <votersAmount>");
      return;
    }

    const votersAmount = parseInt(votersAmountStr, 10) || 5; // Default to 5 if not specified
    if (isNaN(votersAmount) || votersAmount <= 0) {
      console.error("Invalid voters amount. Must be a positive number.");
      return;
    }

    log(`Voting on proposal ${proposalId} with ${votersAmount} accounts...`);
    const success = await voteOnProposal(proposalId, votersAmount);
    log(success ? "Voting completed successfully" : "Voting failed");
  },
};

function showHelp() {
  console.log("Valid commands:");
  console.log("  init         - Create new accounts");
  console.log(
    "  deploy       - Deploy smart contract to first created account",
  );
  console.log(
    "  load         - Initialize deployed contract with configuration and policy",
  );
  console.log("  add_proposal - Add a proposal to the deployed contract");
  console.log(
    "  vote         - Vote on a proposal with specified voter accounts",
  );
  console.log(
    "                 Usage: node script.js vote <proposalId> <votersAmount>",
  );
}

const command = process.argv[2];
const args = process.argv.slice(3);

if (commands[command]) {
  commands[command](...args).catch((err) => {
    console.error(`Error executing ${command}:`, err);
  });
} else {
  showHelp();
}
