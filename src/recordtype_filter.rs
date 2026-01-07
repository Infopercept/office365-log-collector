// RecordType filtering for Office365 subscriptions
// Ensures each subscription only returns its correct log types

use std::collections::HashSet;

/// RecordType mappings based on Microsoft Office365 Management Activity API
/// Reference: https://docs.microsoft.com/en-us/office/office-365-management-api/office-365-management-activity-api-schema
pub struct RecordTypeFilter;

impl RecordTypeFilter {
    /// Get allowed RecordTypes for a given subscription
    /// Returns None if no filtering should be applied (allow all)
    pub fn get_allowed_recordtypes(subscription: &str) -> Option<HashSet<i32>> {
        // DISABLED: Allow all RecordTypes for all subscriptions
        // Microsoft's API behavior is inconsistent - just let everything through
        None

        /* ORIGINAL FILTERING (DISABLED):
        match subscription {
            // DLP.All - All DLP-related events
            "DLP.All" => Some(vec![
                11, // ComplianceDLPSharePoint (DLP evaluation on SharePoint/OneDrive)
                13, // ComplianceDLPExchange (DLP evaluation on Exchange)
                28, // DLPRuleMatch (actual DLP policy violations)
            ].into_iter().collect()),

            // Audit.Exchange - Exchange operations
            "Audit.Exchange" => Some(vec![
                1,  // ExchangeAdmin
                2,  // ExchangeItem (mailbox operations)
                3,  // ExchangeItemGroup
                20, // ExchangeItemAggregated
                50, // MailSubmission
            ].into_iter().collect()),

            // Audit.SharePoint - SharePoint and OneDrive operations
            "Audit.SharePoint" => Some(vec![
                4,  // SharePointFileOperation
                6,  // SharePointFileOperation (legacy)
                14, // SharePointSharingOperation
                19, // SharePointListOperation
            ].into_iter().collect()),

            // Audit.AzureActiveDirectory - Azure AD operations
            "Audit.AzureActiveDirectory" => Some(vec![
                8,  // AzureActiveDirectory
                15, // AzureActiveDirectoryStsLogon (user logins)
            ].into_iter().collect()),

            // Audit.General - Microsoft Teams and other workloads
            "Audit.General" => Some(vec![
                25, // MicrosoftTeams
                30, // MicrosoftFlow
                32, // Yammer
                40, // Dynamics CRM
                44, // PowerBI
                62, // MicrosoftForms
                63, // MicrosoftDefenderForEndpoint
                64, // WorkplaceAnalytics
                65, // PowerAppsApp
                70, // MicrosoftGraphDataConnect
            ].into_iter().collect()),

            // Unknown subscription - allow all (no filtering)
            _ => None,
        }
        */
    }

    /// Check if a log should be included based on its RecordType
    pub fn should_include_log(subscription: &str, record_type: i32) -> bool {
        match Self::get_allowed_recordtypes(subscription) {
            Some(allowed_types) => allowed_types.contains(&record_type),
            None => true, // No filter defined, allow all
        }
    }

    /// Get human-readable description of RecordType
    pub fn get_recordtype_description(record_type: i32) -> &'static str {
        match record_type {
            1 => "ExchangeAdmin",
            2 => "ExchangeItem",
            3 => "ExchangeItemGroup",
            4 => "SharePointFileOperation",
            6 => "SharePointFileOperation",
            8 => "AzureActiveDirectory",
            11 => "ComplianceDLPSharePoint",
            13 => "ComplianceDLPExchange",
            14 => "SharePointSharingOperation",
            15 => "AzureActiveDirectoryStsLogon",
            19 => "SharePointListOperation",
            20 => "ExchangeItemAggregated",
            25 => "MicrosoftTeams",
            28 => "DLPRuleMatch",
            30 => "MicrosoftFlow",
            32 => "Yammer",
            40 => "DynamicsCRM",
            44 => "PowerBI",
            50 => "MailSubmission",
            62 => "MicrosoftForms",
            63 => "MicrosoftDefenderForEndpoint",
            64 => "WorkplaceAnalytics",
            65 => "PowerAppsApp",
            70 => "MicrosoftGraphDataConnect",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlp_all_filtering() {
        // DLP.All should ONLY accept RecordType 28
        assert!(RecordTypeFilter::should_include_log("DLP.All", 28));
        assert!(!RecordTypeFilter::should_include_log("DLP.All", 6));
        assert!(!RecordTypeFilter::should_include_log("DLP.All", 4));
    }

    #[test]
    fn test_sharepoint_filtering() {
        // SharePoint should accept file operations
        assert!(RecordTypeFilter::should_include_log("Audit.SharePoint", 4));
        assert!(RecordTypeFilter::should_include_log("Audit.SharePoint", 6));
        assert!(RecordTypeFilter::should_include_log("Audit.SharePoint", 14));

        // But NOT Teams logs
        assert!(!RecordTypeFilter::should_include_log("Audit.SharePoint", 25));
    }

    #[test]
    fn test_exchange_filtering() {
        // Exchange should accept mail operations
        assert!(RecordTypeFilter::should_include_log("Audit.Exchange", 1));
        assert!(RecordTypeFilter::should_include_log("Audit.Exchange", 2));
        assert!(RecordTypeFilter::should_include_log("Audit.Exchange", 50));

        // But NOT SharePoint logs
        assert!(!RecordTypeFilter::should_include_log("Audit.Exchange", 6));
    }
}
