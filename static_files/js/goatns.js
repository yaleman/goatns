function zoneRecordEditor() {
    const modalEl = document.getElementById("zoneRecordEditModal");
    if (!modalEl) {
        return;
    }

    const form = document.getElementById("zoneRecordEditForm");
    const errorEl = document.getElementById("zoneRecordEditError");
    const saveButton = document.getElementById("zoneRecordEditSave");
    const fields = {
        id: document.getElementById("zoneRecordEditId"),
        name: document.getElementById("zoneRecordEditName"),
        rrtype: document.getElementById("zoneRecordEditType"),
        rclass: document.getElementById("zoneRecordEditClass"),
        ttl: document.getElementById("zoneRecordEditTtl"),
        rdata: document.getElementById("zoneRecordEditData"),
    };

    const modal = new bootstrap.Modal(modalEl);

    const ensureSelectOption = (selectEl, value, labelPrefix) => {
        if (!value) {
            return;
        }
        const escaped = CSS.escape(value);
        if (!selectEl.querySelector(`option[value="${escaped}"]`)) {
            const option = document.createElement("option");
            option.value = value;
            option.textContent = `${labelPrefix} ${value}`;
            selectEl.appendChild(option);
        }
    };

    const openEditor = (row) => {
        const rrtypeValue = row.dataset.recordRrtype || "";
        const rclassValue = row.dataset.recordRclass || "";

        fields.id.value = row.dataset.recordId || "";
        fields.name.value = row.dataset.recordName || "";
        ensureSelectOption(fields.rrtype, rrtypeValue, "Type");
        ensureSelectOption(fields.rclass, rclassValue, "Class");
        fields.rrtype.value = rrtypeValue;
        fields.rclass.value = rclassValue;
        fields.ttl.value = row.dataset.recordTtl || "";
        fields.rdata.value = row.dataset.recordRdata || "";
        errorEl.classList.add("d-none");
        errorEl.textContent = "";
        modal.show();
    };

    document.querySelectorAll("[data-record-edit]").forEach((row) => {
        row.addEventListener("click", (event) => {
            if (event.target.closest("button, a, input, select, textarea")) {
                openEditor(row);
                return;
            }
            openEditor(row);
        });

        row.addEventListener("keydown", (event) => {
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                openEditor(row);
            }
        });
    });

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        saveButton.disabled = true;
        errorEl.classList.add("d-none");
        errorEl.textContent = "";

        const payload = {
            id: fields.id.value,
            name: fields.name.value.trim(),
            rdata: fields.rdata.value.trim(),
        };

        const rrtype = Number.parseInt(fields.rrtype.value, 10);
        if (!Number.isNaN(rrtype)) {
            payload.rrtype = rrtype;
        }

        const rclass = Number.parseInt(fields.rclass.value, 10);
        if (!Number.isNaN(rclass)) {
            payload.rclass = rclass;
        }

        const ttlValue = fields.ttl.value.trim();
        if (ttlValue !== "") {
            const ttl = Number.parseInt(ttlValue, 10);
            if (!Number.isNaN(ttl)) {
                payload.ttl = ttl;
            }
        }

        try {
            const response = await fetch("/api/record", {
                method: "PUT",
                headers: {
                    "Content-Type": "application/json",
                },
                body: JSON.stringify(payload),
            });

            if (!response.ok) {
                const errorPayload = await response.json().catch(() => ({}));
                const message =
                    errorPayload.error ||
                    errorPayload.message ||
                    "Failed to update record.";
                throw new Error(message);
            }

            window.location.reload();
        } catch (error) {
            errorEl.textContent = error.message || "Failed to update record.";
            errorEl.classList.remove("d-none");
        } finally {
            saveButton.disabled = false;
        }
    });
}

document.addEventListener("DOMContentLoaded", () => {
    zoneRecordEditor();
    zoneEditor();
});

function zoneEditor() {
    const modalEl = document.getElementById("zoneEditModal");
    if (!modalEl) {
        return;
    }

    const form = document.getElementById("zoneEditForm");
    const errorEl = document.getElementById("zoneEditError");
    const saveButton = document.getElementById("zoneEditSave");
    const editButton = document.getElementById("zoneEditButton");
    const fields = {
        id: document.getElementById("zoneEditId"),
        rname: document.getElementById("zoneEditRname"),
        serial: document.getElementById("zoneEditSerial"),
        refresh: document.getElementById("zoneEditRefresh"),
        retry: document.getElementById("zoneEditRetry"),
        expire: document.getElementById("zoneEditExpire"),
        minimum: document.getElementById("zoneEditMinimum"),
    };

    const modal = new bootstrap.Modal(modalEl);

    if (editButton) {
        editButton.addEventListener("click", () => {
            errorEl.classList.add("d-none");
            errorEl.textContent = "";
            modal.show();
        });
    }

    const parseRequiredInt = (value, fieldLabel) => {
        const parsed = Number.parseInt(value, 10);
        if (Number.isNaN(parsed)) {
            throw new Error(`${fieldLabel} must be a number.`);
        }
        return parsed;
    };

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        saveButton.disabled = true;
        errorEl.classList.add("d-none");
        errorEl.textContent = "";

        try {
            const payload = {
                id: fields.id.value,
                rname: fields.rname.value.trim(),
                serial: parseRequiredInt(fields.serial.value, "Serial"),
                refresh: parseRequiredInt(fields.refresh.value, "Refresh"),
                retry: parseRequiredInt(fields.retry.value, "Retry"),
                expire: parseRequiredInt(fields.expire.value, "Expire"),
                minimum: parseRequiredInt(fields.minimum.value, "Minimum"),
            };

            if (!payload.rname) {
                throw new Error("Responsible name (RNAME) is required.");
            }

            const response = await fetch("/api/zone", {
                method: "PUT",
                headers: {
                    "Content-Type": "application/json",
                },
                body: JSON.stringify(payload),
            });

            if (!response.ok) {
                const errorPayload = await response.json().catch(() => ({}));
                const message =
                    errorPayload.error ||
                    errorPayload.message ||
                    "Failed to update zone.";
                throw new Error(message);
            }

            window.location.reload();
        } catch (error) {
            errorEl.textContent = error.message || "Failed to update zone.";
            errorEl.classList.remove("d-none");
        } finally {
            saveButton.disabled = false;
        }
    });
}
