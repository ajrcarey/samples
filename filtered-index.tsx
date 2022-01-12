import * as React from 'react';
import * as ReactDOM from 'react-dom';
import { $, numberToWord, capitalizeWord } from '../../utils';
import { makeSticky, stickyMenubarAwareScrollTo } from '../../lib/sticky/sticky';
import { Result } from '../../lib/result/result';
import { Collection } from '../../lib/collection/collection';
import { GroupedCollection } from '../../lib/collection/grouped';

export interface FilteredIndexEntry {
    descriptionInList : number | string,
    descriptionInIndex : number | string
}

export interface FilteredIndexListItem {
    name : string,
    keywordSearchableName : string,
    permalink : string
}

export interface DisplayOption {
    description : string,
    isSelectedByDefault : boolean
}

export interface GroupingOption<T extends Collection<any>, U> {
    description : string,
    provider : (collection : T) => GroupedCollection<FilteredIndexEntry, U>
}

interface FilteredIndexProps<T extends Collection<any>, U, V extends FilteredIndexListItem> {
    list : HTMLElement,
    initializer : () => Promise<T>,
    groupingOptions : GroupingOption<T, U>[],
    displayOptions : DisplayOption[],
    groupCountSingularDescription : string,
    groupCountPluralDescription : string,
    columnsPerListGroup : number,
    filterPlaceholder? : string,
    listItemFormatter : (item : U) => V,
    listItemRenderer? : (item : V,
        index : number,
        selectedDisplayOptions : DisplayOption[]) => JSX.Element,
    groupRenderer? : (key : FilteredIndexEntry,
        filteredValues : V[],
        unfilteredValuesCount : number,
        selectedDisplayOptions : DisplayOption[]) => JSX.Element,
    forceInitialGroupRender? : boolean
}

interface FilteredIndexState<T, V extends FilteredIndexListItem> {
    dataset? : T,
    isFirstRender : boolean,
    currentlySelectedGroupingOptionIndex : number,
    currentlySelectedDisplayOptions : DisplayOption[],
    columnsPerListGroup : number,
    listGroupMinLengthForMultiColumn : number,
    filterPlaceholder : string,
    filterKeywords : string,
    listItemRenderer : (item : V,
        index : number,
        selectedDisplayOptions : DisplayOption[]) => JSX.Element,
    groupRenderer : (key : FilteredIndexEntry,
        filteredValues : V[],
        unfilteredValuesCount : number,
        selectedDisplayOptions : DisplayOption[]) => JSX.Element
}

export class FilteredIndex<T extends Collection<any>, U, V extends FilteredIndexListItem>
extends React.Component<FilteredIndexProps<T, U, V>, FilteredIndexState<T, V>> {

    private static minimumFilterKeywordsLength : number = 2;

    private static displayOptionDisplayTotalInEachGroup : DisplayOption = {
        description: 'Display total in each group',
        isSelectedByDefault: true
    };

    private readonly filterInput : React.RefObject<any>;

    constructor(props : FilteredIndexProps<T, U, V>) {
        super(props);

        this.props.displayOptions.unshift(FilteredIndex.displayOptionDisplayTotalInEachGroup);

        this.state = {
            isFirstRender: this.props.forceInitialGroupRender != true,
            currentlySelectedGroupingOptionIndex: 0,
            currentlySelectedDisplayOptions: this.props.displayOptions.filter(option => option.isSelectedByDefault),
            columnsPerListGroup: this.props.columnsPerListGroup,
            listGroupMinLengthForMultiColumn: 5,
            filterPlaceholder: this.props.filterPlaceholder || 'Type to filter...',
            filterKeywords: '',
            listItemRenderer: this.props.listItemRenderer ||
                (
                    (item, index, selectedDisplayOptions) =>
                        this.defaultListItemRenderer(
                            item,
                            index,
                            selectedDisplayOptions
                        )
                ),
            groupRenderer: this.props.groupRenderer ||
                (
                    (key, filteredValues, unfilteredValuesCount, selectedDisplayOptions) =>
                        this.defaultGroupRenderer(
                            key,
                            filteredValues,
                            unfilteredValuesCount,
                            selectedDisplayOptions
                        )
                )
        };

        this.filterInput = React.createRef();
    }

    componentDidMount() {
        this.props.initializer().then(dataset => {
            this.setState({
                dataset: dataset
            });
        });
    }

    componentDidUpdate() {

        // Refresh the displayed listings now, if the user made a change in the
        // grouping or display options.

        if (!this.state.isFirstRender) {

            // We don't need to refresh the displayed listings after the first paint of
            // this component because the HTML received from the server already contains
            // the correct initial listings. On all other passes, replace the listed content
            // with a freshly generated list that reflects the user's current grouping,
            // filtering, and display options.

            this.getGroupedList().ifSuccess(groupedList => {
                ReactDOM.render(this.renderList(groupedList), this.props.list);
            });
        }

        // Since the number of entries in the index can change, the overall height of
        // the rendered component can change, and therefore the sticky dimensions of
        // the component can change. We therefore re-apply our stickiness after each
        // rendering.

        const e = ReactDOM.findDOMNode(this);

        if (e instanceof HTMLElement) {
            makeSticky(e, null, true);
        }
    }

    private onGroupingOptionClick(index : number) {
        this.setState({
            currentlySelectedGroupingOptionIndex: index,
            isFirstRender: false
        });
    }

    private onDisplayOptionClick(index : number) {

        // Toggle the given display option.

        this.setState(state => {
            const activeIndex = state.currentlySelectedDisplayOptions.indexOf(
                this.props.displayOptions[index]
            );

            const displayOptions = state.currentlySelectedDisplayOptions;

            if (activeIndex == -1) {

                // Activate the display option.

                displayOptions.push(this.props.displayOptions[index]);
            } else {

                // Deactivate the display option.

                displayOptions.splice(activeIndex, 1);
            }

            return {
                currentlySelectedDisplayOptions: displayOptions,
                isFirstRender: false
            };
        });
    }

    private onIndexEntryClick(key : FilteredIndexEntry) {
        stickyMenubarAwareScrollTo($('index-' + key.descriptionInIndex));
    }

    private updateFilter(e : React.ChangeEvent) {
        const keywords : string = (e.target as HTMLInputElement).value;

        this.setState({
            filterKeywords: keywords,
            isFirstRender: false
        });
    }

    private static datasetNotReadyError : Error = new Error('Dataset not ready');

    private getGroupedList() : Result<GroupedCollection<FilteredIndexEntry, V>> {
        if (this.state.dataset) {
            return Result.succeed(
                this.props.groupingOptions[this.state.currentlySelectedGroupingOptionIndex]
                    .provider(this.state.dataset)
                    .mapValues(this.props.listItemFormatter)
            );
        } else {
            return Result.fail(FilteredIndex.datasetNotReadyError);
        }
    }

    render() {
        const groupedList = this.getGroupedList();

        const groupingOptions = this.props.groupingOptions.map((option, index) => {
            return (
                <div key={'grouping-option-' + index.toString()}>
                    <input
                        type="radio"
                        id={'grouping-option-' + index.toString()}
                        name="grouping-option"
                        value={index.toString()}
                        onChange={() => this.onGroupingOptionClick(index)}
                        checked={index == this.state.currentlySelectedGroupingOptionIndex} />
                    <label htmlFor={'grouping-option-' + index.toString()}>
                        {option.description}
                    </label>
                </div>
            );
        });

        const index = groupedList.map<JSX.Element[] | undefined>(
            () => undefined,
            groupedList => groupedList.mapGroups((key, values) =>
                <li key={key.descriptionInIndex}>
                    <a
                        className="sidebar-index-group"
                        onClick={() => this.onIndexEntryClick(key)}>
                        {key.descriptionInIndex}
                    </a>
                    {
                        FilteredIndex.isDisplayOptionSelected(
                            FilteredIndex.displayOptionDisplayTotalInEachGroup,
                            this.state.currentlySelectedDisplayOptions
                        )
                            ? <span className="sidebar-index-group-total">({values.length})</span>
                            : undefined
                    }
                </li>
            )
        );

        const meanIndexEntryLength = groupedList.map(
            () => 0,
            groupedList => groupedList.keys.reduce(
                (acc, key) => acc + key.descriptionInIndex.toString().length,
                0
            ) / groupedList.keysLength
        );

        const indexColumns = groupedList.map(
            () => 'one',
            groupedList => groupedList.keys.length > 12 && meanIndexEntryLength < 7
                ? 'three'
                : groupedList.keys.length > 5
                    ? 'two'
                    : 'one'
        );

        const displayOptions = this.props.displayOptions.map((option, index) => {
            return (
                <div key={'display-option-' + index.toString()}>
                    <input
                        type="checkbox"
                        id={'display-option-' + index.toString()}
                        onChange={e => this.onDisplayOptionClick(index)}
                        checked={FilteredIndex.isDisplayOptionSelected(option, this.state.currentlySelectedDisplayOptions)} />
                    <label htmlFor={'display-option-' + index.toString()}>{option.description}</label>
                </div>
            );
        });

        return (
            <div>
                <h3>Grouping Options</h3>
                <div id="sidebar-grouping-option-list" className="sidebar-radio-list">
                    {groupingOptions}
                </div>
                <h3>Filter</h3>
                <input
                    ref={this.filterInput}
                    type="text"
                    value={this.state.filterKeywords}
                    name="keywords"
                    id="search-keywords"
                    placeholder={this.state.filterPlaceholder}
                    onChange={e => this.updateFilter(e)} />
                <h3>Index</h3>
                <ul className={indexColumns + '-columns sidebar-filter-index'}>
                    {index}
                </ul>
                <h3>Display Options</h3>
                <div className="sidebar-checkbox-list">
                    {displayOptions}
                </div>
            </div>
        );
    }

    private renderList(groupedList : GroupedCollection<FilteredIndexEntry, V>) : JSX.Element {
        return (
            <React.Fragment>
                {
                    groupedList.mapGroups((key, values) => {
                        const filteredValues = this.state.filterKeywords.length > FilteredIndex.minimumFilterKeywordsLength
                            ? values.filter(value =>
                                value.keywordSearchableName.includes(this.state.filterKeywords)
                            )
                            : values;

                        return this.state.groupRenderer(
                            key,
                            filteredValues,
                            values.length,
                            this.state.currentlySelectedDisplayOptions
                        );
                    })
                }
            </React.Fragment>
        );
    }

    private defaultListItemRenderer(value : V, index : number, _ : DisplayOption[]) : JSX.Element {
        return (
            <li key={index}><a href={value.permalink}>{value.name}</a></li>
        );
    }

    private defaultGroupRenderer(
        key : FilteredIndexEntry,
        filteredValues : V[],
        unfilteredValuesCount : number,
        selectedDisplayOptions : DisplayOption[]) : JSX.Element {

        const doDisplayTotalInGroup = FilteredIndex.isDisplayOptionSelected(
            FilteredIndex.displayOptionDisplayTotalInEachGroup,
            selectedDisplayOptions
        );

        const totalInGroup = doDisplayTotalInGroup
            ? <span className="grouped-list-group-total">
                {
                    (unfilteredValuesCount == 1
                        ? 'One ' + this.props.groupCountSingularDescription
                        : capitalizeWord(numberToWord(unfilteredValuesCount)) + ' ' + this.props.groupCountPluralDescription) +
                    ' in group' +
                    (filteredValues.length != unfilteredValuesCount
                        ? ' (' + filteredValues.length.toString() + ' matching filter, ' +
                            (unfilteredValuesCount - filteredValues.length).toString() + ' hidden)'
                        : '')
                }
                </span>
            : undefined;

        const listItems = filteredValues.map((value, index) =>
            this.state.listItemRenderer(value, index, selectedDisplayOptions)
        );

        return (
            <div key={key.descriptionInIndex}>
                <h4 id={'index-' + key.descriptionInIndex}>
                    {key.descriptionInList}
                    {totalInGroup}
                </h4>
                {
                    filteredValues.length > 0
                        ? <ul className={
                            filteredValues.length >= this.state.listGroupMinLengthForMultiColumn && this.state.columnsPerListGroup > 1
                                ? numberToWord(this.state.columnsPerListGroup) + '-columns'
                                : 'one-column'
                        }>
                            {listItems}
                        </ul>
                        : <div />
                }
            </div>
        );
    }

    public static isDisplayOptionSelected(
        option : DisplayOption,
        currentlySelectedDisplayOptions : DisplayOption[]) : boolean {

        return currentlySelectedDisplayOptions.indexOf(option) != -1;
    }

}