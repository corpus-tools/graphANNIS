/*
 * Copyright 2016 Thomas Krause.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.korpling.annis.benchmark.generator;

import annis.ql.parser.AnnisParserAntlr;
import annis.ql.parser.QueryData;
import annis.ql.parser.SemanticValidator;
import com.google.common.base.Joiner;
import com.google.common.io.Files;
import java.io.File;
import java.io.IOException;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.List;
import java.util.ResourceBundle;
import javafx.beans.property.ReadOnlyObjectWrapper;
import javafx.beans.property.SimpleObjectProperty;
import javafx.collections.FXCollections;
import javafx.collections.ListChangeListener.Change;
import javafx.collections.ObservableList;
import javafx.collections.transformation.FilteredList;
import javafx.collections.transformation.SortedList;
import javafx.event.ActionEvent;
import javafx.fxml.FXML;
import javafx.fxml.Initializable;
import javafx.scene.Parent;
import javafx.scene.control.Alert;
import javafx.scene.control.Alert.AlertType;
import javafx.scene.control.ButtonType;
import javafx.scene.control.CheckBox;
import javafx.scene.control.Label;
import javafx.scene.control.TableCell;
import javafx.scene.control.TableColumn;
import javafx.scene.control.TableView;
import javafx.scene.control.TextField;
import javafx.scene.control.cell.PropertyValueFactory;
import javafx.scene.input.Clipboard;
import javafx.scene.input.ClipboardContent;
import javafx.scene.text.Text;
import javafx.stage.DirectoryChooser;
import javafx.stage.FileChooser;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * FXML Controller class
 *
 * @author thomas
 */
public class QuerySetViewController implements Initializable
{

  private final Logger log = LoggerFactory.getLogger(
    QuerySetViewController.class);

  @FXML
  private Parent root;

  private final FileChooser fileChooser = new FileChooser();
  private final DirectoryChooser dirChooser = new DirectoryChooser();
  
  private final FileChooser.ExtensionFilter logFilter = new FileChooser.ExtensionFilter(
    "Query log (*.log)", "*.log");
  

  @FXML
  private TableView<Query> tableView;

  @FXML
  private TableColumn<Query, String> nameColumn;

  @FXML
  private TableColumn<Query, String> aqlColumn;

  @FXML
  private TableColumn<Query, String> corpusColumn;

  @FXML
  private TableColumn<Query, Long> execTimeColumn;
  
  @FXML
  private TableColumn<Query, Boolean> validColumn;

  @FXML
  private TableColumn<Query, Long> nrResultsColumn;

  @FXML
  private TextField corpusFilter;

  @FXML
  private CheckBox oneCorpusFilter;
  
  @FXML
  private CheckBox onlyValidFilter;

  @FXML
  private Label counterLabel;
  

  private final ObservableList<Query> queries = FXCollections.
    observableArrayList();
  
  private AnnisParserAntlr parser;

  /**
   * Initializes the controller class.
   */
  @Override
  public void initialize(URL url, ResourceBundle rb)
  {
    parser = new AnnisParserAntlr();
    parser.setPrecedenceBound(50);
    parser.setPostProcessors(Arrays.asList(new SemanticValidator()));  

    nameColumn.setCellValueFactory(val -> new ReadOnlyObjectWrapper<>(""));
    nameColumn.setCellFactory(param -> new TableCell<Query, String>()
    {
      @Override
      protected void updateItem(String item, boolean empty)
      {
        super.updateItem(item, empty);
        if(this.getTableRow() != null && item != null)
        {
          setText("" + this.getTableRow().getIndex());
        }
        else
        {
          setText("");
        }
      }
    }
    );

    aqlColumn.setCellValueFactory(new PropertyValueFactory<>("aql"));
    corpusColumn.setCellValueFactory(new PropertyValueFactory<>("corpus"));

    corpusColumn.setCellValueFactory(val
      -> new SimpleObjectProperty<>(
        Joiner.on(", ").join(val.getValue().getCorpora())));

    execTimeColumn.setCellValueFactory(param -> new SimpleObjectProperty<>(
      param.getValue().getExecutionTime().orElse(-1l)));

    nrResultsColumn.setCellValueFactory(param -> new SimpleObjectProperty<>(
      param.getValue().getCount().orElse(-1l)));

    aqlColumn.setCellFactory(param
      -> 
      {
        final TableCell cell = new TableCell()
        {
          private Text text;

          @Override
          public void updateItem(Object item, boolean empty)
          {
            super.updateItem(item, empty);
            if (isEmpty())
            {
              setGraphic(null);
            }
            else
            {
              text = new Text(item.toString());
              text.wrappingWidthProperty().bind(widthProperty());
              setGraphic(text);
            }
          }
        };
        return cell;

    });
    validColumn.setCellValueFactory(param -> new ReadOnlyObjectWrapper<>(param.getValue().getJson() != null));

    FilteredList<Query> filteredQueries = new FilteredList<>(queries, p -> true);
    SortedList<Query> sortedQueries = new SortedList<>(filteredQueries);

    sortedQueries.comparatorProperty().bind(tableView.comparatorProperty());

    corpusFilter.textProperty().addListener(observable
      -> 
      {
        setFilterPredicate(filteredQueries);
    });
    oneCorpusFilter.selectedProperty().addListener(observable
      -> 
      {
        setFilterPredicate(filteredQueries);
    });
    onlyValidFilter.selectedProperty().addListener(observable
      -> 
      {
        setFilterPredicate(filteredQueries);
    });
    
    tableView.setItems(sortedQueries);

    filteredQueries.addListener((Change<? extends Query> change)
      -> 
      {
        counterLabel.textProperty().set("" + filteredQueries.size());
    });
  }

  private void setFilterPredicate(FilteredList<Query> filteredQueries)
  {
    if (filteredQueries != null)
    {
      filteredQueries.setPredicate(query
        -> 
        {

          String corpusFilterText = corpusFilter.textProperty().get();
          boolean allowSingleCorpusOnly = oneCorpusFilter.selectedProperty().
            get();
          boolean allowWithJsonOnly = onlyValidFilter.selectedProperty().get();

          if (allowSingleCorpusOnly && query.getCorpora().size() > 1)
          {
            return false;
          }
          
          if(allowWithJsonOnly && query.getJson() == null)
          {
            return false;
          }

          if (corpusFilterText != null && !corpusFilterText.isEmpty())
          {
            if (!query.getCorpora().contains(corpusFilterText))
            {
              return false;
            }
          }

          return true;

      });
    }
  }

  @FXML
  public void filterByCorpusOfQuery(ActionEvent evt)
  {
    Query q = tableView.getSelectionModel().getSelectedItem();
    if (q != null && !q.getCorpora().isEmpty())
    {
      evt.consume();
      oneCorpusFilter.selectedProperty().set(true);
      corpusFilter.textProperty().set(q.getCorpora().iterator().next());
    }
  }
  
  @FXML
  public void copySelectedQuery(ActionEvent evt)
  {
    Query q = tableView.getSelectionModel().getSelectedItem();
    if (q != null && q.getAql() != null)
    {
      evt.consume();
      ClipboardContent content = new ClipboardContent();
      content.putString(q.getAql());
      Clipboard.getSystemClipboard().setContent(content);
    }
  }

  @FXML
  public void loadQueryLog(ActionEvent evt)
  {
    fileChooser.setTitle("Open Query Log");
    fileChooser.getExtensionFilters().clear();
    fileChooser.getExtensionFilters().add(logFilter);
    fileChooser.setSelectedExtensionFilter(logFilter);

    File selectedFile = fileChooser.showOpenDialog(root.getScene().getWindow());
    if (selectedFile != null)
    {
      try
      {
        List<Query> parsedQueries = Files.readLines(selectedFile,
          StandardCharsets.UTF_8,
          new QueryLogParser());

        queries.clear();
        queries.addAll(parsedQueries);

      }
      catch (IOException ex)
      {
        log.error(null, ex);
        new Alert(Alert.AlertType.ERROR, "Could not parse file: " + ex.
          getMessage(), ButtonType.OK).showAndWait();

      }
    }
  }
  
  @FXML
  public void export(ActionEvent evt)
  {
    dirChooser.setTitle("Set export directory");
    
    File dir = dirChooser.showDialog(root.getScene().getWindow());
    if(dir != null)
    {
      int errorCounter = 0;
      int i =1;
      for(Query q : tableView.getItems())
      {
        String name = "" + i++;
        File fAQL = new File(dir, name + ".aql");
        File fJSON = new File(dir, name + ".json");
        
        try
        { 
          Files.write(q.getAql(), fAQL, StandardCharsets.UTF_8);
          Files.write(q.getJson(), fJSON, StandardCharsets.UTF_8);
        }
        catch (Exception ex)
        {
          log.error("not exported: " + q.getAql(), ex);
          errorCounter++;
        }
      }
      if(errorCounter == 0)
      {
        new Alert(AlertType.INFORMATION, "All queries exported successfully", ButtonType.OK).showAndWait();
      }
      else
      {
        new Alert(AlertType.ERROR, "" + errorCounter + " had errors", ButtonType.OK).showAndWait();
      }
    }
  }
  
  @FXML
  public void parseJSON(ActionEvent evt)
  {
    // only parse the visible items
    tableView.getItems().stream().
      forEach((q) ->
    {
      try
      {
        QueryData queryData = parser.parse(q.getAql(), null);
        String asJSON = QueryToJSON.serializeQuery(queryData);
        q.setJson(asJSON);
      }
      catch(Exception ex)
      {
        log.error("Could not create json", ex);
      }
    });
  }

}
