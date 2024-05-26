function validate() {
    var allOk = true;
    var spent = 0.0;

    for (const inputBox of inputBoxes) {
        const span = document.getElementById(inputBox.id.replace("input", "cost"));
        const n = Number(inputBox.value);
        if (Number.isSafeInteger(n) && Number.isSafeInteger(n * n)) {
            spent += n * n;
            if (n == 0) {
                inputBox.classList.remove("nonzero");
                span.innerText = "";
            } else {
                inputBox.classList.add("nonzero");
                span.innerText = `(${n * n} coins)`;
            }
        } else {
            allOk = false;
            span.innerText = "Must be an integer!";
        }
    }

    const coinsLeft = coinsToSpend - spent;

    const coinsLeftSpan = document.getElementById("coins-left");
    const submitButton = document.getElementById("submit-vote");
    coinsLeftSpan.innerText = coinsLeft == 1 ? "1 coin" : `${coinsLeft} coins`;

    submitButton.disabled = (coinsLeft < 0) || !allOk;
}

function initialize() {
    for (const inputBox of inputBoxes) {
        inputBox.addEventListener("input", (event) => {
            validate();
            voteMessage.innerText = (
                "You have unsaved changes. " +
                "Click the button above to submit. " +
                "You can still change your vote after you submit, " +
                "as long as voting is open."
            );
        });
    }
    validate();
}

document.addEventListener("DOMContentLoaded", initialize);
